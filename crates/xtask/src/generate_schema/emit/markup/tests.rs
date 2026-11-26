use alloc::collections::BTreeSet;
use std::path::Path;

use os_shim::mock::MemorySystem;

use super::names::{Overrides, Titles};
use super::render::layout::{self, Expr};
use super::structs::{Attribute, Body, Catalog, Child, Node, Target, Value};
use super::{Crate, Data, assign, enums, namespaces, render};
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

/// The namespaces of the twin data set: `w`, the standard's `wp`, and Word's `wps`.
const WORD_NAMESPACES: &str = r#"[
  {"Prefix": "w", "Uri": "urn:w"},
  {"Prefix": "wp", "Uri": "urn:wp"},
  {"Prefix": "wps", "Uri": "urn:wps"}
]"#;

/// The standard's `wp:shape`, with an optional, a required and a repeated child, and
/// `wp:group`, whose choice holds `wp:shape`.
const WORD_DRAWING: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="urn:wp" targetNamespace="urn:wp" elementFormDefault="qualified">
  <xsd:complexType name="CT_Props"/>
  <xsd:complexType name="CT_Text"/>
  <xsd:complexType name="CT_Shape">
    <xsd:sequence>
      <xsd:element name="props" type="CT_Props" minOccurs="0"/>
      <xsd:element name="body" type="CT_Text"/>
      <xsd:element name="note" type="CT_Text" minOccurs="0" maxOccurs="unbounded"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Group">
    <xsd:choice minOccurs="0" maxOccurs="unbounded">
      <xsd:element ref="shape"/>
      <xsd:element name="props" type="CT_Props"/>
    </xsd:choice>
  </xsd:complexType>
  <xsd:element name="shape" type="CT_Shape"/>
  <xsd:element name="group" type="CT_Group"/>
</xsd:schema>"#;

/// Word's `wps:shape`, the data's only form of the standard's `wp:shape`, its note in `w`.
const WORD_DATA: &str = r#"{"TargetNamespace": "urn:wps", "Types": [
  {"Name": "wps:CT_Shape/wps:shape",
   "Children": [{"Name": "wps:CT_Props/wps:props"}, {"Name": "wps:CT_Text/wps:body"}, {"Name": "w:CT_Text/w:note"}],
   "Particle": {"Kind": "Sequence", "Items": [
     {"Name": "wps:CT_Props/wps:props", "Occurs": [{"Max": 1}]},
     {"Name": "wps:CT_Text/wps:body"},
     {"Name": "w:CT_Text/w:note", "Occurs": [{}]}
   ]}},
  {"Name": "wps:CT_Props/wps:props"},
  {"Name": "wps:CT_Text/wps:body"},
  {"Name": "w:CT_Text/w:note"},
  {"Name": "wps:CT_Holder/wps:holder"}
]}"#;

/// The twin of `wps:shape` in `wp`, whose note Word writes `w:note`.
const WORD_TWIN: &str = r#"
[[twin]]
roots = ["wps:CT_Shape/wps:shape"]
from = ["wps"]
prefix = "wp"
parent = "wps:CT_Holder/wps:holder"
also = [["wps:CT_Shape/wps:shape", "wp:note", "w:note"]]
"#;

const WORD_ENUMS: &str = r#"
[[enum]]
module = "wordprocessing_drawing"
name = "GroupContent"
source = "local choice in CT_Group"
types = ["CT_Group"]
"#;

/// `wp:ext`, whose one required child is any element, and `wp:pick`, whose one required child
/// is the empty `wp:second` or `wp:first`, whose two attributes make it larger than the inline
/// bytes.
const REQUIRED_DRAWING: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="urn:wp" targetNamespace="urn:wp" elementFormDefault="qualified">
  <xsd:complexType name="CT_Empty"/>
  <xsd:complexType name="CT_Large">
    <xsd:attribute name="a" type="xsd:string"/>
    <xsd:attribute name="b" type="xsd:string"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Extension">
    <xsd:sequence>
      <xsd:any processContents="lax"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Pick">
    <xsd:choice>
      <xsd:element name="second" type="CT_Empty"/>
      <xsd:element name="first" type="CT_Large"/>
    </xsd:choice>
  </xsd:complexType>
  <xsd:element name="ext" type="CT_Extension"/>
  <xsd:element name="pick" type="CT_Pick"/>
</xsd:schema>"#;

const REQUIRED_ENUMS: &str = r###"
[[enum]]
module = "wordprocessing_drawing"
name = "AnyElementContent"
source = "an xsd:any of any namespace"
wildcard = "##any"

[[enum]]
module = "wordprocessing_drawing"
name = "PickContent"
source = "local choice in CT_Pick"
types = ["CT_Pick"]
"###;

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

/// The typed tree of the twin data set, built as `emit` builds it: from the data as read, with
/// the names of the corrected data's twins.
fn word_catalog() -> Catalog {
    let system = MemorySystem::new()
        .with_file("/data/namespaces.json", WORD_NAMESPACES.as_bytes())
        .unwrap()
        .with_file("/data/typed/namespaces.json", b"[]")
        .unwrap()
        .with_file("/data/parts/none.json", br#"{"Name": "None"}"#)
        .unwrap()
        .with_file("/data/schematrons.json", b"[]")
        .unwrap()
        .with_file("/data/schemas/wps.json", WORD_DATA.as_bytes())
        .unwrap()
        .with_file(
            "/xsd/dml-wordprocessingDrawing.xsd",
            WORD_DRAWING.as_bytes(),
        )
        .unwrap();
    let sdk = SdkData::read(&system, Path::new("data")).unwrap();
    let transitional = SchemaSet::read(&system, &["/xsd/dml-wordprocessingDrawing.xsd"]).unwrap();
    let mut corrected = sdk.clone();
    corrected.correct(WORD_TWIN, &transitional).unwrap();
    let strict = SchemaSet::default();
    let rows = namespaces::rows(&sdk, &transitional, &strict, &SchemaSet::default()).unwrap();
    let mut data = Data::new(&sdk, &transitional);
    data.aliases = &corrected.aliases;
    let mut catalog = Catalog::build(
        &data,
        &strict,
        &rows,
        &Titles::read("", ""),
        &Overrides::parse("").unwrap(),
        &enums::rows(WORD_ENUMS).unwrap(),
    )
    .unwrap();
    catalog.crates = assign(&render::references(&catalog).unwrap());
    catalog
}

/// The rendered `wordprocessing_drawing` module of the set whose required positions are a
/// wildcard and a choice.
fn required_module() -> String {
    let system = MemorySystem::new()
        .with_file("/data/namespaces.json", WORD_NAMESPACES.as_bytes())
        .unwrap()
        .with_file("/data/typed/namespaces.json", b"[]")
        .unwrap()
        .with_file("/data/parts/none.json", br#"{"Name": "None"}"#)
        .unwrap()
        .with_file("/data/schematrons.json", b"[]")
        .unwrap()
        .with_file(
            "/data/schemas/wp.json",
            br#"{"TargetNamespace": "urn:wp", "Types": []}"#,
        )
        .unwrap()
        .with_file(
            "/xsd/dml-wordprocessingDrawing.xsd",
            REQUIRED_DRAWING.as_bytes(),
        )
        .unwrap();
    let sdk = SdkData::read(&system, Path::new("data")).unwrap();
    let transitional = SchemaSet::read(&system, &["/xsd/dml-wordprocessingDrawing.xsd"]).unwrap();
    let strict = SchemaSet::default();
    let rows = namespaces::rows(&sdk, &transitional, &strict, &SchemaSet::default()).unwrap();
    let mut catalog = Catalog::build(
        &Data::new(&sdk, &transitional),
        &strict,
        &rows,
        &Titles::read("", ""),
        &Overrides::parse("").unwrap(),
        &enums::rows(REQUIRED_ENUMS).unwrap(),
    )
    .unwrap();
    catalog.crates = assign(&render::references(&catalog).unwrap());
    let files = render::files(&MemorySystem::new(), "", &catalog).unwrap();
    files
        .into_iter()
        .find(|(path, _)| path.ends_with("wordprocessing_drawing.rs"))
        .map(|(_, text)| text)
        .unwrap()
}

/// The struct of the tag `wp:local`.
fn word_node<'catalog>(catalog: &'catalog Catalog, local: &str) -> &'catalog Node {
    catalog
        .nodes
        .iter()
        .find(|node| node.key.tag.local == local)
        .unwrap()
}

/// The children of a struct that holds children.
fn children_of(node: &Node) -> Option<&[Child]> {
    let Body::Children(children) = &node.body else {
        return None;
    };
    Some(children)
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
fn a_twin_struct_takes_its_own_other_names_and_each_child_those_under_it() {
    let catalog = word_catalog();
    let id = |prefix: &str| {
        catalog
            .prefixes
            .iter()
            .position(|found| found == prefix)
            .map(|index| u8::try_from(index).unwrap())
    };
    let shape = word_node(&catalog, "shape");
    assert_eq!(shape.also, [(id("wps"), "shape".to_owned())]);
    let children = children_of(shape).unwrap();
    let also: Vec<_> = children
        .iter()
        .map(|child| (child.field.as_str(), child.also.clone()))
        .collect();
    assert_eq!(
        also,
        [
            ("props", vec![(id("wps"), "props".to_owned())]),
            ("body", vec![(id("wps"), "body".to_owned())]),
            ("note", vec![(id("w"), "note".to_owned())]),
        ]
    );
    let group = word_node(&catalog, "group");
    assert_eq!(group.also, []);
    let group_children = children_of(group).unwrap();
    assert_eq!(group_children.len(), 1);
    assert!(matches!(group_children[0].target, Target::Enum(_)));
    assert_eq!(group_children[0].also, []);
}

#[test]
fn the_module_reads_each_position_and_arm_under_its_other_names() {
    let catalog = word_catalog();
    let files = render::files(&MemorySystem::new(), "", &catalog).unwrap();
    let (_, text) = files
        .iter()
        .find(|(path, _)| path.ends_with("wordprocessing_drawing.rs"))
        .unwrap();
    for expected in [
        "const NS_W: Option<NamespaceId> = Some(0);",
        "const NS_WPS: Option<NamespaceId> = Some(2);",
        "Slot::single_also((NS_WP, \"props\"), &[(NS_WPS, \"props\")])",
        "Slot::required_also((NS_WP, \"body\"), &[(NS_WPS, \"body\")])",
        "Slot::list_also((NS_WP, \"note\"), &[(NS_W, \"note\")])",
        "Some((NS_WP | NS_WPS, \"shape\")) =>",
        "Some((NS_WP | NS_WPS, \"props\")) =>",
    ] {
        assert!(text.contains(expected), "{expected} is not in\n{text}");
    }
    let tags = text
        .split("pub static GROUP_CONTENT_TAGS")
        .nth(1)
        .and_then(|rest| rest.split("};").next())
        .unwrap();
    assert!(tags.contains("(NS_WP, \"shape\"),"), "{tags}");
    assert!(tags.contains("(NS_WPS, \"shape\"),"), "{tags}");
    assert!(tags.contains("(NS_WPS, \"props\"),"), "{tags}");
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

#[test]
fn a_required_wildcard_defaults_to_an_empty_other_and_a_required_choice_to_its_first_arm() {
    let text = required_module();
    for expected in [
        "impl<X: UserData> Default for AnyElementContentWith<X> {\n    fn default() -> Self {\n        \
         Self::Other(Box::new(Node::Text(String::new())))\n    }\n}\n",
        "impl<X: UserData> Default for PickContentWith<X> {\n    fn default() -> Self {\n        \
         Self::First(Box::default())\n    }\n}\n",
    ] {
        assert!(text.contains(expected), "{expected} is not in\n{text}");
    }
}

#[test]
fn an_arm_larger_than_the_inline_bytes_is_boxed_built_from_its_struct_and_bounded() {
    let text = required_module();
    for expected in [
        "    First(Box<FirstWith<X>>),\n",
        "    Second(SecondWith<X>),\n",
        "    Other(Box<Node>),\n",
        "impl<X: UserData> From<FirstWith<X>> for PickContentWith<X> {\n    \
         fn from(arm: FirstWith<X>) -> Self {\n        \
         Self::First(Box::new(arm))\n    }\n}\n",
        "const _: () = assert!(\n    size_of::<PickContent>() <= 24,\n    \
         \"an arm the generator holds in place takes more than 16 bytes\",\n);\n",
    ] {
        assert!(text.contains(expected), "{expected} is not in\n{text}");
    }
}
