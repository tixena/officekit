use os_shim::mock::MemorySystem;

use super::{checks, strict_checks};
use crate::generate_schema::emit::validation::{
    Bounds, Check, StringKind, ValueCheck, ValueClass, Versions,
};
use crate::generate_schema::model::{Name, SchemaSet, TypeRef};

const STRICT_W: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

const PERCENT: &str = "-?[0-9]+(\\.[0-9]+)?%";

const POSITIVE_FIXED: &str = "((100)|([0-9][0-9]?))(\\.[0-9][0-9]?)?%";

const MEASURE: &str = "-?[0-9]+(\\.[0-9]+)?(mm|cm|in|pt|pc|pi)";

/// Excerpts of the Strict simple types as ECMA-376 writes them
/// (`shared-commonSimpleTypes.xsd`, `dml-main.xsd`, `pml.xsd`, `wml.xsd`),
/// gathered in one namespace, with a list, bounds, and a type of a built-in
/// the conversion has no check for added.
const STRICT_TYPES: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://purl.oclc.org/ooxml/wordprocessingml/main"
  targetNamespace="http://purl.oclc.org/ooxml/wordprocessingml/main"
  elementFormDefault="qualified" attributeFormDefault="qualified">
  <xsd:simpleType name="ST_OnOff">
    <xsd:union memberTypes="xsd:boolean"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_Percentage">
    <xsd:restriction base="xsd:string">
      <xsd:pattern value="-?[0-9]+(\.[0-9]+)?%"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_PositiveFixedPercentage">
    <xsd:restriction base="ST_Percentage">
      <xsd:pattern value="((100)|([0-9][0-9]?))(\.[0-9][0-9]?)?%"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_UniversalMeasure">
    <xsd:restriction base="xsd:string">
      <xsd:pattern value="-?[0-9]+(\.[0-9]+)?(mm|cm|in|pt|pc|pi)"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_MeasurementOrPercent">
    <xsd:union memberTypes="ST_Percentage ST_UniversalMeasure"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_TLTimeIndefinite">
    <xsd:restriction base="xsd:token">
      <xsd:enumeration value="indefinite"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_TLTimeAnimateValueTime">
    <xsd:union memberTypes="ST_PositiveFixedPercentage ST_TLTimeIndefinite"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_JcTable">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="center"/>
      <xsd:enumeration value="end"/>
      <xsd:enumeration value="start"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_Coordinates">
    <xsd:list itemType="xsd:int"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_Size">
    <xsd:restriction base="xsd:unsignedInt">
      <xsd:minInclusive value="1"/>
      <xsd:maxInclusive value="100"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_Period">
    <xsd:restriction base="xsd:duration"/>
  </xsd:simpleType>
</xsd:schema>"#;

fn every_target(check: Check) -> ValueCheck {
    ValueCheck {
        check,
        union: None,
        versions: Versions::ALL,
    }
}

fn pattern(pattern: &str) -> Check {
    Check::String {
        class: Some(ValueClass::String),
        kind: StringKind::Plain,
        length: None,
        max_length: None,
        min_length: None,
        pattern: Some(pattern.to_owned()),
    }
}

/// The checks of the Strict type `local` of `STRICT_TYPES`.
fn strict_type(local: &str) -> anyhow::Result<Vec<ValueCheck>> {
    let system = MemorySystem::new().with_file("s/wml.xsd", STRICT_TYPES.as_bytes())?;
    let set = SchemaSet::read(&system, &["s/wml.xsd"])?;
    checks(
        &set,
        &TypeRef::Simple {
            anonymous: false,
            name: Name::new(STRICT_W, local),
        },
    )
}

fn union(check: Check, id: u8) -> ValueCheck {
    ValueCheck {
        check,
        union: Some(id),
        versions: Versions::ALL,
    }
}

/// `wml.xsd` of either class, cut to `CT_OnOff`, whose `val` Transitional
/// widens with `on` and `off`, and `CT_Ind`, whose `start` both classes type alike.
fn wml(namespace: &str, transitional: bool) -> String {
    let (members, on_off) = if transitional {
        (
            "xsd:boolean ST_OnOff1",
            r#"<xsd:simpleType name="ST_OnOff1">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="on"/>
      <xsd:enumeration value="off"/>
    </xsd:restriction>
  </xsd:simpleType>"#,
        )
    } else {
        ("xsd:boolean", "")
    };
    format!(
        r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns="{namespace}"
  targetNamespace="{namespace}" elementFormDefault="qualified" attributeFormDefault="qualified">
  <xsd:simpleType name="ST_OnOff">
    <xsd:union memberTypes="{members}"/>
  </xsd:simpleType>
  {on_off}
  <xsd:simpleType name="ST_SignedTwipsMeasure">
    <xsd:restriction base="xsd:integer"/>
  </xsd:simpleType>
  <xsd:complexType name="CT_OnOff">
    <xsd:attribute name="val" type="ST_OnOff"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Ind">
    <xsd:attribute name="start" type="ST_SignedTwipsMeasure"/>
  </xsd:complexType>
</xsd:schema>"#
    )
}

#[test]
fn a_restriction_with_a_pattern_is_its_pattern() {
    assert_eq!(
        strict_type("ST_Percentage").unwrap(),
        [every_target(pattern(PERCENT))],
        "the check of xsd:string passes every value and is left out"
    );
    assert_eq!(
        strict_type("ST_PositiveFixedPercentage").unwrap(),
        [
            every_target(pattern(PERCENT)),
            every_target(pattern(POSITIVE_FIXED)),
        ],
        "the base's checks, then the restriction's pattern"
    );
}

#[test]
fn a_boolean_is_an_enumeration_check_of_its_four_values() {
    assert_eq!(
        strict_type("ST_OnOff").unwrap(),
        [every_target(Check::Enum {
            class: ValueClass::Boolean,
        })],
        "a union of one member is that member"
    );
}

#[test]
fn a_union_puts_its_members_under_one_union_id() {
    assert_eq!(
        strict_type("ST_MeasurementOrPercent").unwrap(),
        [union(pattern(PERCENT), 0), union(pattern(MEASURE), 0)]
    );
}

#[test]
fn a_union_member_of_several_checks_is_distributed_over_the_others() {
    let token = Check::String {
        class: Some(ValueClass::String),
        kind: StringKind::Token,
        length: None,
        max_length: None,
        min_length: None,
        pattern: None,
    };
    assert_eq!(
        strict_type("ST_TLTimeAnimateValueTime").unwrap(),
        [
            union(pattern(PERCENT), 0),
            union(token.clone(), 0),
            union(pattern(PERCENT), 1),
            union(pattern("indefinite"), 1),
            union(pattern(POSITIVE_FIXED), 2),
            union(token, 2),
            union(pattern(POSITIVE_FIXED), 3),
            union(pattern("indefinite"), 3),
        ],
        "(a and b) or (c and d) as (a or c), (a or d), (b or c), (b or d)"
    );
}

#[test]
fn an_enumeration_is_a_pattern_of_its_values() {
    assert_eq!(
        strict_type("ST_JcTable").unwrap(),
        [every_target(pattern("center|end|start"))]
    );
}

#[test]
fn a_list_is_a_list_check_of_its_item_type() {
    assert_eq!(
        strict_type("ST_Coordinates").unwrap(),
        [every_target(Check::List {
            item: Box::new(Check::Number {
                bounds: Bounds::default(),
                class: ValueClass::Int32,
                list: false,
            }),
        })]
    );
}

#[test]
fn bounds_are_a_number_check_after_the_class_check() {
    let number = |bounds: Bounds| Check::Number {
        bounds,
        class: ValueClass::UInt32,
        list: false,
    };
    assert_eq!(
        strict_type("ST_Size").unwrap(),
        [
            every_target(number(Bounds::default())),
            every_target(number(Bounds {
                max_inclusive: Some(100),
                min_inclusive: Some(1),
                ..Bounds::default()
            })),
        ]
    );
}

#[test]
fn a_type_of_a_built_in_with_no_check_stops_the_conversion_naming_it() {
    let error = strict_type("ST_Period").unwrap_err();
    assert!(format!("{error:#}").contains("xsd:duration"), "{error:#}");
}

#[test]
fn only_an_attribute_whose_strict_type_differs_gets_strict_checks() {
    let system = MemorySystem::new()
        .with_file("t/wml.xsd", wml(W, true).as_bytes())
        .unwrap()
        .with_file("s/wml.xsd", wml(STRICT_W, false).as_bytes())
        .unwrap();
    let transitional = SchemaSet::read(&system, &["t/wml.xsd"]).unwrap();
    let strict = SchemaSet::read(&system, &["s/wml.xsd"]).unwrap();
    let found = strict_checks(&transitional, &strict).unwrap();
    let names: Vec<(&str, &str)> = found
        .iter()
        .flat_map(|(complex_type, attributes)| {
            attributes
                .keys()
                .map(|attribute| (complex_type.local.as_str(), attribute.local.as_str()))
        })
        .collect();
    assert_eq!(names, [("CT_OnOff", "val")]);
    assert_eq!(
        found[&Name::new(W, "CT_OnOff")][&Name::new(W, "val")],
        [every_target(Check::Enum {
            class: ValueClass::Boolean,
        })]
    );
}
