use alloc::collections::BTreeSet;
use std::path::Path;

use os_shim::mock::MemorySystem;

use super::names::{Overrides, Titles};
use super::render::layout::{self, Expr};
use super::structs::{Attribute, Body, Catalog, Value};
use super::{Crate, Data, assign, namespaces};
use crate::generate_schema::model::SchemaSet;
use crate::generate_schema::sdk_data::SdkData;

const NAMESPACES: &str = r#"[
  {"Prefix": "x", "Uri": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"},
  {"Prefix": "xml", "Uri": "http://www.w3.org/XML/1998/namespace"}
]"#;

/// `x:t` and `x:v` of the simple type `sml.xsd` gives them, and `x:plain` and `x:lone` of the same.
const SML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
  elementFormDefault="qualified"
  targetNamespace="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <xsd:simpleType name="ST_Xstring">
    <xsd:restriction base="xsd:string"/>
  </xsd:simpleType>
  <xsd:element name="t" type="ST_Xstring"/>
  <xsd:element name="v" type="ST_Xstring"/>
  <xsd:element name="plain" type="ST_Xstring"/>
  <xsd:element name="lone" type="ST_Xstring"/>
</xsd:schema>"#;

/// `x:t` and `x:v` derive from a base with `xml:space`, `x:plain` from a base with no attribute,
/// and `x:lone` from a base the data has no row for.
const SML_DATA: &str = r#"{"TargetNamespace": "http://schemas.openxmlformats.org/spreadsheetml/2006/main", "Types": [
  {"Name": "x:CT_Xstring/", "IsLeafText": true, "Attributes": [{"QName": "xml:space", "Type": "EnumValue<DocumentFormat.OpenXml.SpaceProcessingModeValues>"}]},
  {"Name": "x:CT_Xstring/x:t", "IsDerived": true, "IsLeafText": true},
  {"Name": "x:CT_Xstring/x:v", "IsDerived": true, "IsLeafText": true},
  {"Name": "x:CT_Plain/", "IsLeafText": true},
  {"Name": "x:CT_Plain/x:plain", "IsDerived": true, "IsLeafText": true},
  {"Name": "x:CT_Lone/x:lone", "IsDerived": true, "IsLeafText": true}
]}"#;

fn sml_catalog() -> Catalog {
    let system = MemorySystem::new()
        .with_file("/data/namespaces.json", NAMESPACES.as_bytes())
        .unwrap()
        .with_file("/data/typed/namespaces.json", b"[]")
        .unwrap()
        .with_file("/data/parts/none.json", br#"{"Name": "None"}"#)
        .unwrap()
        .with_file("/data/schematrons.json", b"[]")
        .unwrap()
        .with_file("/data/schemas/sml.json", SML_DATA.as_bytes())
        .unwrap()
        .with_file("/xsd/sml.xsd", SML.as_bytes())
        .unwrap();
    let sdk = SdkData::read(&system, Path::new("data")).unwrap();
    let transitional = SchemaSet::read(&system, &["/xsd/sml.xsd"]).unwrap();
    let strict = SchemaSet::default();
    let rows = namespaces::rows(&sdk, &transitional, &strict, &SchemaSet::default()).unwrap();
    Catalog::build(
        &Data::new(&sdk, &transitional),
        &strict,
        &rows,
        &Titles::read("", ""),
        &Overrides::parse("").unwrap(),
        &[],
    )
    .unwrap()
}

fn text_node_attributes(catalog: &Catalog, tag: &str) -> Vec<Attribute> {
    let node = catalog
        .nodes
        .iter()
        .find(|node| node.key.tag.local == tag)
        .unwrap();
    assert!(matches!(node.body, Body::Text(_)));
    node.attributes.clone()
}

fn call(name: &str, arguments: &[&str]) -> Expr {
    Expr::call(
        name,
        arguments.iter().map(|text| Expr::atom(text)).collect(),
    )
}

#[test]
fn a_statement_that_fits_stays_on_one_line() {
    let mut out = String::new();
    layout::statement(
        "    let mut writer = ",
        &call(
            "crate::Writer::new",
            &["scope", "&super::P_SHAPE", "&node.unknown"],
        ),
        &mut out,
    );
    assert_eq!(
        out,
        "    let mut writer = crate::Writer::new(scope, &super::P_SHAPE, &node.unknown);\n"
    );
}

#[test]
fn a_nested_call_overflows_as_rustfmt_writes_it() {
    let member = Expr::call(
        "PropertySetCustomizedValueMember::PercentageValueWithSign",
        vec![Expr::atom("value")],
    );
    let mut out = String::new();
    layout::line(
        "            return ",
        &Expr::call("Ok", vec![member]),
        ";",
        &mut out,
    );
    assert_eq!(
        out,
        "            return Ok(PropertySetCustomizedValueMember::PercentageValueWithSign(\n                \
         value,\n            ));\n"
    );
}

#[test]
fn a_long_method_call_keeps_its_one_argument_on_the_line() {
    let arm = layout::arm(
        12,
        &Expr::atom("7"),
        &call(
            "field.child",
            &["&mut node.c_nv_grp_sp_pr_non_visual_group_drawing_shape_props"],
        ),
        true,
    );
    assert_eq!(
        arm,
        "            7 => field.child(&mut node.c_nv_grp_sp_pr_non_visual_group_drawing_shape_props),\n"
    );
}

#[test]
fn a_lone_long_array_item_moves_to_the_next_line() {
    let mut out = String::new();
    layout::statement(
        "static DEPTH_PERCENT_WITH_SYMBOL_PATTERNS: &[&str] = ",
        &Expr::Slice(vec![Expr::atom(
            "\"0*(([2-9][0-9])|([1-9][0-9][0-9])|(1[0-9][0-9][0-9])|2000)%\"",
        )]),
        &mut out,
    );
    assert_eq!(
        out,
        "static DEPTH_PERCENT_WITH_SYMBOL_PATTERNS: &[&str] =\n    \
         &[\"0*(([2-9][0-9])|([1-9][0-9][0-9])|(1[0-9][0-9][0-9])|2000)%\"];\n"
    );
}

#[test]
fn literals_read_as_clippy_asks() {
    assert_eq!(layout::integer(0xFFFF_FFFF), "0xFFFF_FFFF");
    assert_eq!(layout::integer(-0x8000_0000), "-0x8000_0000");
    assert_eq!(layout::integer(0x7FFF), "0x7FFF");
    assert_eq!(layout::integer(27_273_042), "27_273_042");
    assert_eq!(layout::integer(1000), "1000");
    assert_eq!(layout::real(1000.0), "1000.0_f64");
    assert_eq!(layout::real(250_000.5), "250_000.5_f64");
    assert_eq!(layout::real(-0.5), "-0.5_f64");
}

#[test]
fn doc_words_that_read_as_code_get_backticks() {
    assert_eq!(
        layout::doc(0, "The HLinks of CT_VectorVariant and w:pPr stay words."),
        "/// The `HLinks` of `CT_VectorVariant` and `w:pPr` stay words.\n"
    );
}

#[test]
fn square_brackets_in_doc_words_are_escaped_so_they_do_not_read_as_links() {
    assert_eq!(
        layout::doc(0, "Blank [False] State"),
        "/// Blank \\[False\\] State\n"
    );
}

#[test]
fn a_long_condition_puts_its_brace_on_a_line_of_its_own() {
    let pattern = Expr::pattern(
        "super::document_properties_custom::CustomFilePropertyContent::BinaryStorage",
        "arm",
    );
    assert_eq!(
        layout::if_let(4, &pattern, "*item"),
        "    if let super::document_properties_custom::CustomFilePropertyContent::BinaryStorage(arm) = *item\n    {\n"
    );
}

#[test]
fn a_method_call_as_a_nested_calls_lone_argument_takes_rustfmts_comma() {
    let mut out = String::new();
    layout::line(
        "        return ",
        &Expr::call(
            "Some",
            vec![Expr::call(
                "super::Model3DExtensionContent::ArcSegment",
                vec![Expr::call("arm.transform", vec![Expr::atom("ctx")])],
            )],
        ),
        ";",
        &mut out,
    );
    assert_eq!(
        out,
        "        return Some(super::Model3DExtensionContent::ArcSegment(\n            arm.transform(ctx),\n        ));\n"
    );
}

#[test]
fn a_simple_typed_element_takes_the_attributes_of_its_base_in_the_data() {
    let catalog = sml_catalog();
    let xml = catalog
        .prefixes
        .iter()
        .position(|prefix| prefix == "xml")
        .map(|index| u8::try_from(index).unwrap());
    for tag in ["t", "v"] {
        assert_eq!(
            text_node_attributes(&catalog, tag),
            [Attribute {
                field: "space".to_owned(),
                local: "space".to_owned(),
                namespace: xml,
                value: Value::Text,
            }]
        );
    }
}

#[test]
fn a_simple_typed_element_with_no_such_base_in_the_data_takes_no_attribute() {
    let catalog = sml_catalog();
    assert_eq!(text_node_attributes(&catalog, "plain"), []);
    assert_eq!(text_node_attributes(&catalog, "lone"), []);
}

#[test]
fn the_cycle_is_core_what_it_reaches_base_and_every_other_module_ext() {
    // 1, 2 and 3 name each other in a cycle; 0 names the cycle, the cycle
    // names 4, which names 5; 6 and 7 name each other, a smaller cycle; 8
    // names nothing and nothing names it.
    let references: Vec<BTreeSet<usize>> =
        [&[1][..], &[2], &[3, 4], &[1], &[5], &[], &[7], &[6], &[]]
            .into_iter()
            .map(|named| named.iter().copied().collect())
            .collect();
    assert_eq!(
        assign(&references),
        [
            Crate::Ext,
            Crate::Core,
            Crate::Core,
            Crate::Core,
            Crate::Base,
            Crate::Base,
            Crate::Ext,
            Crate::Ext,
            Crate::Ext,
        ]
    );
}
