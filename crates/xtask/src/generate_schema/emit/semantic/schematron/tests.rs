use alloc::collections::BTreeMap;

use super::{Names, read};
use crate::generate_schema::emit::semantic::Constraint;
use crate::generate_schema::model::Name;

const O: &str = "urn:schemas-microsoft-com:office:office";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

fn parsed(test: &str) -> Constraint {
    let uris = uris();
    read(test, &Names { uris: &uris }).unwrap()
}

fn texts(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn uris() -> BTreeMap<String, String> {
    [("o", O), ("w", W), ("x", X)]
        .into_iter()
        .map(|(prefix, uri)| (prefix.to_owned(), uri.to_owned()))
        .collect()
}

fn w(local: &str) -> Name {
    Name::new(W, local)
}

fn x(local: &str) -> Name {
    Name::new(X, local)
}

#[test]
fn a_test_of_an_attribute_alone_reads_it_with_no_namespace() {
    assert_eq!(
        parsed("@w:type"),
        Constraint::CannotBeOmitted {
            attribute: Name::new("", "type")
        }
    );
}

#[test]
fn an_office_prefix_on_an_attribute_is_no_namespace() {
    assert_eq!(
        parsed("@ovml:facet >= 1 and @ovml:facet <= 65536"),
        Constraint::ValueRange {
            attribute: Name::new("", "facet"),
            max: 65_536.0_f64,
            max_inclusive: true,
            min: 1.0_f64,
            min_inclusive: true,
        }
    );
}

#[test]
fn ranges_take_their_bounds_from_either_side() {
    assert_eq!(
        parsed("@w:val <= 9"),
        Constraint::ValueRange {
            attribute: w("val"),
            max: 9.0_f64,
            max_inclusive: true,
            min: f64::NEG_INFINITY,
            min_inclusive: true,
        }
    );
    assert_eq!(
        parsed("@w:val > 0 and @w:val < 0x80000000"),
        Constraint::ValueRange {
            attribute: w("val"),
            max: 2_147_483_648.0_f64,
            max_inclusive: false,
            min: 0.0_f64,
            min_inclusive: false,
        }
    );
    assert_eq!(
        parsed("@x:degree >= -32767f"),
        Constraint::ValueRange {
            attribute: x("degree"),
            max: f64::INFINITY,
            max_inclusive: true,
            min: -32_767.0_f64,
            min_inclusive: true,
        }
    );
}

#[test]
fn lengths_and_patterns_read() {
    assert_eq!(
        parsed("string-length(@w:val) >= 1 and string-length(@w:val) <= 31"),
        Constraint::ValueLength {
            attribute: w("val"),
            max: 31,
            min: 1,
        }
    );
    assert_eq!(
        parsed("string-length(@w:val) <= 253"),
        Constraint::ValueLength {
            attribute: w("val"),
            max: 253,
            min: 0,
        }
    );
    assert_eq!(
        parsed(r#"matches(@x:name, "[^\'*]{1}")"#),
        Constraint::ValuePattern {
            attribute: x("name"),
            pattern: "^[^'*]{1}$".to_owned(),
        }
    );
}

#[test]
fn value_sets_allow_or_refuse() {
    assert_eq!(
        parsed("@w:val = 1 or @w:val = 'two'"),
        Constraint::ValueSet {
            attribute: w("val"),
            valid: true,
            values: texts(&["1", "two"]),
        }
    );
    assert_eq!(
        parsed("@x:v != INF and @x:v != -INF and @x:v != NaN"),
        Constraint::ValueSet {
            attribute: x("v"),
            valid: false,
            values: texts(&["INF", "-INF", "NaN"]),
        }
    );
}

#[test]
fn conditions_between_two_attributes_read() {
    assert_eq!(
        parsed("@x:a and (@x:t = icon or @x:t = value)"),
        Constraint::AbsentWhenValue {
            attribute: x("a"),
            other: x("t"),
            values: texts(&["icon", "value"]),
        }
    );
    assert_eq!(
        parsed("@x:a and @x:t != range"),
        Constraint::AbsentUnlessValue {
            attribute: x("a"),
            other: x("t"),
            values: texts(&["range"]),
        }
    );
    assert_eq!(
        parsed("(@x:si and @x:t = shared) or @x:t != shared"),
        Constraint::RequiredWhenValue {
            attribute: x("si"),
            other: x("t"),
            value: "shared".to_owned(),
        }
    );
    assert_eq!(
        parsed(
            "((@w:val = 11 or @w:val = 12) and @w:name = compatibilityMode) or \
             @w:name != compatibilityMode"
        ),
        Constraint::ValueConditioned {
            attribute: w("val"),
            other: w("name"),
            other_values: texts(&["compatibilityMode"]),
            values: texts(&["11", "12"]),
        }
    );
    assert_eq!(
        parsed("@x:sb < @x:eb"),
        Constraint::LessOrEqual {
            attribute: x("sb"),
            can_equal: false,
            other: x("eb"),
        }
    );
}

#[test]
fn exclusive_attributes_are_listed_once_each() {
    assert_eq!(
        parsed("(@x:a and @x:b) or (@x:a and @x:c) or (@x:b and @x:c)"),
        Constraint::MutuallyExclusive {
            attributes: vec![x("a"), x("b"), x("c")],
        }
    );
    assert_eq!(
        parsed("@x:l and @x:s"),
        Constraint::MutuallyExclusive {
            attributes: vec![x("l"), x("s")],
        }
    );
}

#[test]
fn uniqueness_reads_its_case_and_scope() {
    assert_eq!(
        parsed(
            "count(distinct-values(//w:footnotes/w:footnote/@w:id)) = \
             count(//w:footnotes/w:footnote/@w:id)"
        ),
        Constraint::UniqueValue {
            attribute: w("id"),
            case_sensitive: true,
            parent: None,
        }
    );
    assert_eq!(
        parsed(
            "count(distinct-values(lower-case(ancestor::x:table//x:tableColumn/@x:name))) = \
             count(lower-case(ancestor::x:table//x:tableColumn/@x:name))"
        ),
        Constraint::UniqueValue {
            attribute: x("name"),
            case_sensitive: false,
            parent: Some(x("table")),
        }
    );
}

#[test]
fn references_into_parts_and_relationships_read() {
    assert_eq!(
        parsed("Index-of(document('Part:FootnotesPart')//w:footnotes/w:footnote/@w:id, @w:id)"),
        Constraint::ReferenceExists {
            attribute: w("id"),
            element: w("footnote"),
            element_attribute: w("id"),
            element_name: "w:footnote".to_owned(),
            part: "FootnotesPart".to_owned(),
        }
    );
    assert_eq!(
        parsed(
            "@x:s < count(document('Part:/WorkbookPart/WorkbookStylesPart')//x:cellXfs/x:xf) + 0"
        ),
        Constraint::IndexReference {
            attribute: x("s"),
            element: x("xf"),
            index_base: 0,
            parent: Some(x("cellXfs")),
            part: "/WorkbookPart/WorkbookStylesPart".to_owned(),
        }
    );
    assert_eq!(
        parsed("document(rels)//r:Relationship[@Id = current()/@w:id]"),
        Constraint::RelationshipExists { attribute: w("id") }
    );
    assert_eq!(
        parsed(
            "document(rels)//r:Relationship[@Id = current()/@w:id]/@Type = 'http://example.com/font'"
        ),
        Constraint::RelationshipType {
            attribute: w("id"),
            relationship_type: "http://example.com/font".to_owned(),
        }
    );
}

#[test]
fn a_test_of_no_known_form_is_refused() {
    let uris = uris();
    let names = Names { uris: &uris };
    let refused = |test: &str| read(test, &names).unwrap_err().to_string();
    assert_eq!(
        refused("@w:a + @w:b"),
        "the test @w:a + @w:b goes on after its expression"
    );
    assert_eq!(
        refused("(@w:a and (@w:b = 1 or @w:b = 2)) or @w:b != 1"),
        "an attribute required on 2 values"
    );
    assert_eq!(refused("@w:a = 'open"), "an unclosed quote in @w:a = 'open");
}
