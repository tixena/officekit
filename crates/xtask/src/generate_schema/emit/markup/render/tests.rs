use alloc::collections::BTreeSet;

use super::super::archive::Cycles;
use super::super::structs::{
    Attribute, Body, Catalog, Child, EnumDef, EnumKind, Module, Node, Occurs, Target, Value,
};
use super::super::{Crate, NodeKey, Source};
use super::{enum_into_user_data, functions, into_user_data_text, without_children_text};
use crate::generate_schema::model::Name;

/// A struct with the attributes `val` and `alignment`, in schema order.
fn node(body: Body) -> Node {
    let attribute = |field: &str| Attribute {
        field: field.to_owned(),
        local: field.to_owned(),
        namespace: None,
        value: Value::Text,
    };
    Node {
        also: Vec::new(),
        attributes: vec![attribute("val"), attribute("alignment")],
        body,
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

#[test]
fn without_children_copies_the_attributes_and_leaves_each_declared_child_at_its_default() {
    let sample = node(Body::Children(vec![Child {
        also: Vec::new(),
        field: "properties".to_owned(),
        occurs: Occurs::Optional,
        target: Target::Node(0),
    }]));
    assert_eq!(
        without_children_text(&sample),
        "\nimpl<X: UserData> SampleWith<X> {\n    \
         /// This node's attributes, copied, with no declared child and no unknown content.\n    \
         #[must_use = \"this returns a copy; the node is unchanged\"]\n    \
         pub fn without_children(&self) -> Self {\n        \
         Self {\n            \
         alignment: self.alignment.clone(),\n            \
         val: self.val.clone(),\n            \
         ..Self::default()\n        \
         }\n    \
         }\n}\n"
    );
}

#[test]
fn without_children_copies_a_text_value_beside_the_attributes() {
    assert_eq!(
        without_children_text(&node(Body::Text(Value::Text))),
        "\nimpl<X: UserData> SampleWith<X> {\n    \
         /// This node's attributes and value, copied, with no unknown content.\n    \
         #[must_use = \"this returns a copy; the node is unchanged\"]\n    \
         pub fn without_children(&self) -> Self {\n        \
         Self {\n            \
         alignment: self.alignment.clone(),\n            \
         val: self.val.clone(),\n            \
         value: self.value.clone(),\n            \
         ..Self::default()\n        \
         }\n    \
         }\n}\n"
    );
}

/// `Sample`, with an attribute and a child of each occurrence, and `Props`, beside the enum
/// `SampleContent` with an arm of each.
fn into_user_data_catalog() -> Catalog {
    let mut sample = node(Body::Children(vec![
        Child {
            also: Vec::new(),
            field: "content".to_owned(),
            occurs: Occurs::List,
            target: Target::Enum(0),
        },
        Child {
            also: Vec::new(),
            field: "properties".to_owned(),
            occurs: Occurs::Optional,
            target: Target::Node(1),
        },
        Child {
            also: Vec::new(),
            field: "required".to_owned(),
            occurs: Occurs::Required,
            target: Target::Node(1),
        },
    ]));
    sample.attributes.truncate(1);
    let mut props = node(Body::Children(Vec::new()));
    props.attributes.clear();
    props.name = "Props".to_owned();
    Catalog {
        crates: vec![Crate::Ext],
        enums: vec![EnumDef {
            arms: vec![("Props".to_owned(), 1), ("Sample".to_owned(), 0)],
            boxed: BTreeSet::from([0]),
            default: false,
            doc: String::new(),
            kind: EnumKind::Choice,
            module: 0,
            name: "SampleContent".to_owned(),
            wildcard: None,
        }],
        modules: vec![Module {
            id: 0,
            name: "x".to_owned(),
            prefix: "x".to_owned(),
            uri: "urn:x".to_owned(),
        }],
        nodes: vec![sample, props],
        ..Catalog::default()
    }
}

#[test]
fn into_user_data_moves_the_fields_and_converts_each_child_by_its_occurrence() {
    let catalog = into_user_data_catalog();
    let names = functions(&catalog, 0).unwrap();
    let cycles = Cycles::new(&catalog);
    let mut helpers = String::new();
    let text = into_user_data_text(&names, &cycles, 0, &catalog.nodes[0], &mut helpers);
    assert_eq!(helpers, "");
    assert_eq!(
        text,
        "\n    \
         /// This node and every node under it with each `user_data` at `Y`'s default; every other field\n    \
         /// moves as it is.\n    \
         #[must_use = \"the tree moves into the one returned; dropping it drops the tree\"]\n    \
         pub fn into_user_data<Y>(mut self) -> SampleWith<Y>\n    \
         where\n        \
         Y: UserData,\n    \
         {\n        \
         SampleWith {\n            \
         content: officekit_markup_base::into_user_data_list(\n                \
         self.content,\n                \
         SampleContentWith::into_user_data,\n            \
         ),\n            \
         properties: officekit_markup_base::into_user_data_optional(\n                \
         &mut self.properties,\n                \
         PropsWith::into_user_data,\n            \
         ),\n            \
         required: officekit_markup_base::into_user_data_required(\n                \
         *self.required,\n                \
         PropsWith::into_user_data,\n            \
         ),\n            \
         unknown: self.unknown,\n            \
         user_data: Default::default(),\n            \
         val: self.val,\n        \
         }\n    \
         }\n"
    );
}

#[test]
fn into_user_data_of_an_enum_converts_each_arm_and_moves_other() {
    let catalog = into_user_data_catalog();
    let names = functions(&catalog, 0).unwrap();
    assert_eq!(
        enum_into_user_data(&catalog.enums[0], names.runtime()),
        "\nimpl<X: UserData> SampleContentWith<X> {\n    \
         /// This item and every node under it with each `user_data` at `Y`'s default.\n    \
         #[must_use = \"the tree moves into the one returned; dropping it drops the tree\"]\n    \
         pub fn into_user_data<Y>(self) -> SampleContentWith<Y>\n    \
         where\n        \
         Y: UserData,\n    \
         {\n        \
         match self {\n            \
         Self::Other(node) => SampleContentWith::Other(node),\n            \
         Self::Props(node) => SampleContentWith::Props(node.into_user_data()),\n            \
         Self::Sample(node) => SampleContentWith::Sample(Box::new((*node).into_user_data())),\n        \
         }\n    \
         }\n}\n"
    );
}

#[test]
fn into_user_data_of_a_struct_past_the_body_lines_moves_its_fields_in_helpers() {
    let mut catalog = into_user_data_catalog();
    catalog.nodes[0].attributes = (0_u32..100)
        .map(|number| Attribute {
            field: format!("field_{number:03}"),
            local: format!("field{number}"),
            namespace: None,
            value: Value::Text,
        })
        .collect();
    let names = functions(&catalog, 0).unwrap();
    let cycles = Cycles::new(&catalog);
    let mut helpers = String::new();
    let text = into_user_data_text(&names, &cycles, 0, &catalog.nodes[0], &mut helpers);
    assert!(text.contains("pub fn into_user_data<Y>(mut self) -> SampleWith<Y>"));
    assert!(text.contains("let mut node = SampleWith::<Y>::default();"));
    assert!(text.contains("into_user_data_sample_1(&mut self, &mut node);"));
    assert!(text.contains("into_user_data_sample_2(&mut self, &mut node);"));
    assert!(helpers.contains(
        "fn into_user_data_sample_1<X, Y>(from: &mut SampleWith<X>, node: &mut SampleWith<Y>)\nwhere\n    X: UserData,"
    ));
    assert!(helpers.contains("    node.field_000 = mem::take(&mut from.field_000);\n"));
    assert!(helpers.contains(
        "    node.properties = officekit_markup_base::into_user_data_optional(\n        &mut from.properties,\n        PropsWith::into_user_data,\n    );\n"
    ));
    assert!(!helpers.contains("from.user_data"));
}

#[test]
fn into_user_data_of_an_enum_past_the_body_lines_is_the_runtimes_match() {
    let mut catalog = into_user_data_catalog();
    let mut arms = Vec::new();
    for number in 0_u32..120 {
        arms.push((format!("Arm{number:03}"), 1));
    }
    catalog.enums[0].arms = arms;
    catalog.enums[0].boxed = BTreeSet::from([0]);
    let text = enum_into_user_data(&catalog.enums[0], "officekit_markup_base");
    assert!(text.contains("officekit_markup_base::into_user_data_match!(self, SampleContentWith;\n            plain [\n                Arm000, Arm001,"));
    assert!(text.contains("            ]\n            boxed [\n            ]\n        )\n"));
}
