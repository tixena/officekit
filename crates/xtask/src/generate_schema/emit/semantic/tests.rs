use alloc::collections::BTreeMap;

use serde_json::{Value, json};

use super::{
    Constraint, Registration, Source, applications, attribute_slot, registrations, rust_pattern,
};
use crate::generate_schema::emit::validation::Versions;
use crate::generate_schema::emit::validation::render::{self, Expr};
use crate::generate_schema::model::Name;
use crate::generate_schema::sdk_data::{DataNamespace, SdkData};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The data's namespaces, `w` alone, and `rules`, each a context, a test,
/// and an application.
fn data(rules: &[(&str, &str, &str)]) -> SdkData {
    SdkData {
        enumerations: BTreeMap::default(),
        namespaces: vec![DataNamespace {
            prefix: "w".to_owned(),
            uri: W.to_owned(),
            version: None,
        }],
        parts: Vec::new(),
        schematrons: rules
            .iter()
            .map(|(context, test, app)| json!({"Context": context, "Test": test, "App": app}))
            .collect(),
        typed_namespaces: BTreeMap::default(),
        types: BTreeMap::default(),
    }
}

/// A dump class of `w:local` with `constraints`.
fn class(name: &str, local: &str, constraints: &[Value]) -> Value {
    json!({"class": name, "namespace": W, "local": local, "constraints": constraints})
}

/// A dump registration of the unique-value class on `w:id`.
fn unique(application: &str) -> Value {
    json!({
        "kind": "UniqueAttributeValueConstraint",
        "version": "Office2007",
        "application": application,
        "fields": {"_attribute": {"namespace": W, "local": "id"}, "_parent": null, "_comparer": "Ordinal"},
    })
}

fn unique_on_id() -> Constraint {
    Constraint::UniqueValue {
        attribute: Name::new(W, "id"),
        case_sensitive: true,
        parent: None,
    }
}

#[test]
fn equal_constraints_of_one_element_are_registered_once() {
    let rules = data(&[
        (
            "w:moveFrom",
            "count(distinct-values(//w:a/w:moveFrom/@w:id)) = count(//w:a/w:moveFrom/@w:id)",
            "All",
        ),
        (
            "w:moveFrom",
            "count(distinct-values(//w:b/w:moveFrom/@w:id)) = count(//w:b/w:moveFrom/@w:id)",
            "Word",
        ),
    ]);
    let dump = json!([
        class("MoveFrom", "moveFrom", &[unique("All")]),
        class("MoveFromRun", "moveFrom", &[unique("All")])
    ]);
    let found = registrations(&rules, &dump).unwrap();
    let registered = &found[&Name::new(W, "moveFrom")];
    assert_eq!(registered.classes, 2);
    assert_eq!(
        registered.registrations,
        [(
            Registration {
                applications: 0b111,
                constraint: unique_on_id(),
                versions: Versions::ALL,
            },
            Source::Rule(0)
        )]
    );
}

#[test]
fn a_registration_no_rule_gives_comes_from_the_dump() {
    let dump = json!([class("Comment", "comment", &[unique("Word")])]);
    let found = registrations(&data(&[]), &dump).unwrap();
    assert_eq!(
        found[&Name::new(W, "comment")].registrations,
        [(
            Registration {
                applications: 0b001,
                constraint: unique_on_id(),
                versions: Versions::ALL,
            },
            Source::Dump
        )]
    );
}

#[test]
fn a_rule_the_dump_lacks_is_refused() {
    let rules = data(&[("w:comment", "@w:id >= 0", "All")]);
    let other = json!([class("Comment", "comment", &[unique("All")])]);
    for dump in [other, json!([])] {
        let message = registrations(&rules, &dump).unwrap_err().to_string();
        assert!(message.contains("SDK 3.5.1 does not register"), "{message}");
    }
}

#[test]
fn two_classes_of_one_element_must_agree() {
    let dump = json!([
        class("A", "comment", &[unique("All")]),
        class("B", "comment", &[unique("Word")])
    ]);
    assert_eq!(
        registrations(&data(&[]), &dump).unwrap_err().to_string(),
        format!("two classes of {{{W}}}comment differ in their registrations")
    );
}

#[test]
fn applications_read_as_a_mask() {
    assert_eq!(applications("All").unwrap(), 0b111);
    assert_eq!(applications("Word, Excel").unwrap(), 0b011);
    assert_eq!(applications("PowerPoint").unwrap(), 0b100);
    assert_eq!(
        applications("Visio").unwrap_err().to_string(),
        "unknown application Visio"
    );
}

#[test]
fn a_slot_is_the_exact_name_else_the_last_of_the_local_name() {
    let attributes = [
        Name::new("", "id"),
        Name::new(W, "val"),
        Name::new("urn:other", "val"),
    ];
    assert_eq!(
        attribute_slot(&attributes, &Name::new(W, "val")).unwrap(),
        Some(1)
    );
    assert_eq!(
        attribute_slot(&attributes, &Name::new("", "val")).unwrap(),
        Some(2)
    );
    assert_eq!(
        attribute_slot(&attributes, &Name::new(W, "id")).unwrap(),
        Some(0)
    );
    assert_eq!(
        attribute_slot(&attributes, &Name::new(W, "name")).unwrap(),
        None
    );
}

#[test]
fn a_net_block_becomes_the_range_past_ascii() {
    assert_eq!(
        rust_pattern("^[\\p{L}\\P{IsBasicLatin}]*$").unwrap(),
        "^[\\p{L}\\x{80}-\\x{10FFFF}]*$"
    );
    assert_eq!(
        rust_pattern("^\\p{IsGreek}$").unwrap_err().to_string(),
        "the pattern ^\\p{IsGreek}$ names a .NET block"
    );
}

#[test]
fn reals_and_short_arrays_render_as_rustfmt_writes_them() {
    let mut text = String::new();
    render::statement(
        "const A: [f64; 5] = ",
        &Expr::Slice(vec![
            render::real(f64::INFINITY),
            render::real(f64::NEG_INFINITY),
            render::real(2_147_483_648.0),
            render::real(-0.5),
            render::real(1.7e308),
        ]),
        &mut text,
    );
    assert_eq!(
        text,
        "const A: [f64; 5] = &[\n    f64::INFINITY,\n    f64::NEG_INFINITY,\n    \
         2_147_483_648.0_f64,\n    -0.5_f64,\n    1.7e308_f64,\n];\n"
    );
    let mut values = String::new();
    let numbers = (1_u8..=30_u8)
        .map(|number| render::string(&number.to_string()))
        .collect();
    render::statement("const B: &[&str] = ", &Expr::Slice(numbers), &mut values);
    assert_eq!(
        values,
        "const B: &[&str] = &[\n    \"1\", \"2\", \"3\", \"4\", \"5\", \"6\", \"7\", \"8\", \"9\", \
         \"10\", \"11\", \"12\", \"13\", \"14\", \"15\", \"16\", \"17\",\n    \"18\", \"19\", \
         \"20\", \"21\", \"22\", \"23\", \"24\", \"25\", \"26\", \"27\", \"28\", \"29\", \"30\",\n];\n"
    );
}
