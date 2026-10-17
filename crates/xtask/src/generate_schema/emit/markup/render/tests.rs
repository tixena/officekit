use super::super::structs::{Attribute, Body, Child, Node, Occurs, Target, Value};
use super::super::{NodeKey, Source};
use super::without_children_text;
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
        field: "properties".to_owned(),
        occurs: Occurs::Optional,
        target: Target::Node(0),
    }]));
    assert_eq!(
        without_children_text(&sample),
        "\nimpl Sample {\n    \
         /// This node's attributes, copied, with no declared child and no unknown content.\n    \
         #[must_use]\n    \
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
        "\nimpl Sample {\n    \
         /// This node's attributes and value, copied, with no unknown content.\n    \
         #[must_use]\n    \
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
