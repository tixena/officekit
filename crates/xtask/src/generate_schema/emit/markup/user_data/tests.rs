use alloc::collections::BTreeSet;

use super::super::structs::{Attribute, Body, Catalog, EnumDef, EnumKind, Module, Node, Value};
use super::super::{NodeKey, Source};
use super::{check, kinds_text, name, text};
use crate::generate_schema::model::Name;

fn module(name: &str) -> Module {
    Module {
        id: 0,
        name: name.to_owned(),
        prefix: "x".to_owned(),
        uri: "urn:x".to_owned(),
    }
}

fn node(module: usize, name: &str) -> Node {
    Node {
        also: Vec::new(),
        attributes: Vec::new(),
        body: Body::Children(Vec::new()),
        doc: String::new(),
        key: NodeKey {
            source: Source::Data(format!("x:CT_{name}/x:{name}")),
            tag: Name::new("urn:x", name),
        },
        module,
        name: name.to_owned(),
        namespace: None,
        section: None,
        words: Vec::new(),
    }
}

fn catalog(modules: &[&str], nodes: Vec<Node>) -> Catalog {
    Catalog {
        modules: modules.iter().map(|name| module(name)).collect(),
        nodes,
        ..Catalog::default()
    }
}

#[test]
fn the_associated_type_is_the_module_and_the_struct_in_upper_camel_case() {
    let catalog = catalog(
        &["wordprocessing", "drawing_text"],
        vec![node(0, "Paragraph"), node(1, "Paragraphs")],
    );
    let shown: Vec<String> = catalog
        .nodes
        .iter()
        .map(|node| name(&catalog, node))
        .collect();
    assert_eq!(shown, ["WordprocessingParagraph", "DrawingTextParagraphs"]);
}

#[test]
fn two_structs_with_one_associated_type_name_are_refused() {
    let catalog = catalog(&["a_b", "a"], vec![node(0, "C"), node(1, "BC")]);
    let error = check(&catalog).unwrap_err().to_string();
    assert_eq!(error, "C and BC both take the associated type ABC");
}

#[test]
fn a_generic_type_named_like_another_type_of_its_module_is_refused() {
    let mut catalog = catalog(&["a"], vec![node(0, "Run"), node(0, "RunWith")]);
    let error = check(&catalog).unwrap_err().to_string();
    assert_eq!(error, "RunWith of a is also the name of another type");
    catalog.nodes.pop();
    catalog.enums.push(EnumDef {
        arms: Vec::new(),
        boxed: BTreeSet::new(),
        default: false,
        doc: String::new(),
        kind: EnumKind::Choice,
        module: 0,
        name: "RunWith".to_owned(),
        wildcard: None,
    });
    assert!(check(&catalog).is_err());
}

#[test]
fn a_field_named_user_data_is_refused() {
    let mut found = node(0, "Run");
    found.attributes.push(Attribute {
        field: "user_data".to_owned(),
        local: "user_data".to_owned(),
        namespace: None,
        value: Value::Text,
    });
    let error = check(&catalog(&["a"], vec![found]))
        .unwrap_err()
        .to_string();
    assert_eq!(error, "a::Run has a field named user_data");
}

#[test]
fn the_trait_has_one_associated_type_for_each_struct_in_alphabetical_order() {
    let catalog = catalog(
        &["wordprocessing", "drawing"],
        vec![node(0, "Text"), node(0, "Paragraph"), node(1, "Line")],
    );
    let text = text("// header\n", &catalog).unwrap();
    let declared: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("    type ") && line.ends_with(": UserDataSlot;"))
        .collect();
    assert_eq!(
        declared,
        [
            "    type DrawingLine: UserDataSlot;",
            "    type WordprocessingParagraph: UserDataSlot;",
            "    type WordprocessingText: UserDataSlot;",
        ]
    );
    assert!(text.contains("    /// What a `wordprocessing::ParagraphWith` holds.\n"));
    assert!(text.contains("impl UserData for () {\n    type DrawingLine = ();\n"));
}

#[test]
fn the_list_of_kinds_is_the_trait_s_associated_types_in_its_order() {
    let catalog = catalog(
        &["wordprocessing", "drawing"],
        vec![node(0, "Text"), node(0, "Paragraph"), node(1, "Line")],
    );
    let declared: Vec<String> = text("// header\n", &catalog)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("    type "))
        .filter_map(|line| line.strip_suffix(": UserDataSlot;"))
        .map(str::to_owned)
        .collect();
    let listed: Vec<String> = kinds_text("// header\n", &catalog)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("    \""))
        .filter_map(|line| line.strip_suffix("\","))
        .map(str::to_owned)
        .collect();
    assert_eq!(declared.len(), 3);
    assert_eq!(listed, declared);
}
