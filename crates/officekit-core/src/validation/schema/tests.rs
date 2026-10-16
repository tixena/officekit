use alloc::collections::BTreeSet;

use super::matches_pattern;
use super::types::{
    Bounds, Check, CompositeKind, Particle, SchemaType, StringKind, ValueCheck, ValueClass,
    Versions,
};
use crate::validation::generated::{ENUMERATIONS, PART_RULES, SCHEMA_TYPES, type_id};
use crate::xml::name::table::NAMESPACES;

const ALL: Versions = Versions(0b11_1111);

const NO_BOUNDS: Bounds = Bounds {
    max_exclusive: None,
    max_inclusive: None,
    min_exclusive: None,
    min_inclusive: None,
    non_negative: false,
    positive: false,
};

/// A composite's kind, its bounds, and its element items by type name and bounds.
type Composite = (CompositeKind, u32, u32, Vec<(&'static str, u32, u32)>);

/// The type of `parent`'s child `local`, through the parent's `children`.
fn child(parent: &str, local: &str) -> &'static SchemaType {
    let (_, _, id, _) = schema_type(parent)
        .children
        .iter()
        .find(|(_, name, _, _)| *name == local)
        .unwrap();
    &SCHEMA_TYPES[usize::from(*id)]
}

fn composite(particle: &Particle) -> Option<Composite> {
    if let Particle::Composite {
        items,
        kind,
        max,
        min,
    } = *particle
    {
        Some((kind, min, max, items.iter().filter_map(element).collect()))
    } else {
        None
    }
}

fn element(particle: &Particle) -> Option<(&'static str, u32, u32)> {
    if let Particle::Element { max, min, type_id } = *particle {
        Some((SCHEMA_TYPES[usize::from(type_id)].name, min, max))
    } else {
        None
    }
}

const fn every_target(check: Check) -> ValueCheck {
    ValueCheck {
        check,
        union: None,
        versions: ALL,
    }
}

const fn number(class: ValueClass, min_inclusive: Option<f64>) -> Check {
    Check::Number {
        bounds: Bounds {
            min_inclusive,
            ..NO_BOUNDS
        },
        class,
        list: false,
    }
}

fn schema_type(name: &str) -> &'static SchemaType {
    &SCHEMA_TYPES[usize::from(type_id(name).unwrap())]
}

const fn string(pattern: Option<&'static str>) -> Check {
    Check::String {
        class: None,
        kind: StringKind::Plain,
        length: None,
        max_length: None,
        min_length: None,
        pattern,
    }
}

fn width(parent: &str, local: &str) -> &'static [ValueCheck] {
    let attribute = child(parent, local)
        .attributes
        .iter()
        .find(|attribute| attribute.name == "w")
        .unwrap();
    attribute.checks
}

#[test]
fn the_sample_of_the_spec_reads_the_left_margins_width() {
    let margins = &SCHEMA_TYPES[usize::from(type_id("w:CT_TblCellMar/w:tblCellMar").unwrap())];
    let left = margins
        .children
        .iter()
        .find(|(_, local, _, _)| *local == "left")
        .unwrap()
        .2;
    let width = SCHEMA_TYPES[usize::from(left)]
        .attributes
        .iter()
        .find(|a| a.name == "w")
        .unwrap();
    assert!(matches!(width.class, ValueClass::Int16));
    assert!(matches!(width.checks[0].check, Check::Required));
    assert_eq!(
        NAMESPACES[usize::from(width.namespace.unwrap())].prefix,
        "w"
    );
}

#[test]
fn the_left_and_right_margins_take_a_required_non_negative_int16() {
    for side in ["left", "right"] {
        assert_eq!(
            width("w:CT_TblCellMar/w:tblCellMar", side),
            [
                every_target(Check::Required),
                every_target(number(ValueClass::Int16, Some(0.0_f64))),
                every_target(number(ValueClass::Int16, None)),
            ],
            "{side}"
        );
    }
}

#[test]
fn the_top_and_bottom_margins_take_a_union_per_version() {
    let percent = string(Some("-?[0-9]+(\\.[0-9]+)?%"));
    let int32 = number(ValueClass::Int32, None);
    let check = |check: Check, union: Option<u8>, versions: u8| ValueCheck {
        check,
        union,
        versions: Versions(versions),
    };
    for side in ["top", "bottom"] {
        assert_eq!(
            width("w:CT_TblCellMar/w:tblCellMar", side),
            [
                check(int32, None, 0b00_0000),
                check(percent, Some(0), 0b00_0001),
                check(int32, Some(0), 0b00_0001),
                check(percent, Some(1), 0b11_1110),
                check(int32, Some(1), 0b11_1110),
                every_target(string(None)),
            ],
            "{side}"
        );
    }
}

#[test]
fn the_margins_have_one_content_model_with_start_and_end() {
    let models: Vec<(Versions, Option<_>)> = schema_type("w:CT_TblCellMar/w:tblCellMar")
        .particles
        .iter()
        .map(|(versions, model)| (*versions, model.as_ref().and_then(composite)))
        .collect();
    assert_eq!(
        models,
        [(
            ALL,
            Some((
                CompositeKind::Sequence,
                1,
                1,
                vec![
                    ("w:CT_TblWidth/w:top", 0, 1),
                    ("w:CT_TblWidthDxaNil/w:left", 0, 1),
                    ("w:CT_TblWidth/w:start", 0, 1),
                    ("w:CT_TblWidth/w:bottom", 0, 1),
                    ("w:CT_TblWidthDxaNil/w:right", 0, 1),
                    ("w:CT_TblWidth/w:end", 0, 1),
                ]
            ))
        )]
    );
}

#[test]
fn transitional_only_left_and_right_are_marked() {
    let marked: Vec<&str> = schema_type("w:CT_TblCellMar/w:tblCellMar")
        .children
        .iter()
        .filter(|(_, _, _, transitional_only)| *transitional_only)
        .map(|(_, local, _, _)| *local)
        .collect();
    assert_eq!(marked, ["left", "right"]);
    let justification = ENUMERATIONS
        .iter()
        .find(|enumeration| enumeration.simple_type == "w:ST_Jc")
        .unwrap();
    let values: Vec<&str> = justification
        .members
        .iter()
        .filter(|(_, _, transitional_only)| *transitional_only)
        .map(|(value, _, _)| *value)
        .collect();
    assert_eq!(values, ["left", "right"]);
}

#[test]
fn a_part_rule_names_its_root_type() {
    let main = PART_RULES
        .iter()
        .find(|rule| rule.name == "MainDocumentPart")
        .unwrap();
    assert_eq!(main.root, type_id("w:CT_Document/w:document"));
}

#[test]
fn every_pattern_of_the_tables_compiles_and_matches_whole() {
    let mut patterns = BTreeSet::new();
    for row in SCHEMA_TYPES {
        for check in row
            .attributes
            .iter()
            .flat_map(|attribute| attribute.checks)
            .chain(row.text_checks)
        {
            if let Check::String {
                class: _class,
                kind: _kind,
                length: _length,
                max_length: _max_length,
                min_length: _min_length,
                pattern: Some(pattern),
            } = check.check
            {
                patterns.insert(pattern);
            }
        }
    }
    assert_eq!(patterns.len(), 14);
    for pattern in patterns {
        assert!(matches_pattern(pattern, "").is_ok(), "{pattern}");
    }
    let percent = "-?[0-9]+(\\.[0-9]+)?%";
    assert!(matches_pattern(percent, "10%").unwrap());
    assert!(matches_pattern(percent, "-2.5%").unwrap());
    assert!(!matches_pattern(percent, "x10%").unwrap());
    assert!(!matches_pattern(percent, "10%x").unwrap());
    assert!(!matches_pattern(percent, "100.0").unwrap());
}
