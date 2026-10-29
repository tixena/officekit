use std::path::Path;

use os_shim::mock::MemorySystem;

use super::SdkData;
use crate::generate_schema::model::SchemaSet;

const NAMESPACES: &str = r#"[{"Prefix": "", "Uri": ""}]"#;

const ATTRIBUTE_ROW: &str = r#"
[[attribute]]
type = "x:CT_Inner/x:inner"
replaces = "x:val"
qname = ":val"
"#;

const CHART: &str = r#"{
  "TargetNamespace": "chart",
  "Types": [
    {"Name": "c:CT_List/c:extLst", "Children": [{"Name": "x:CT_Wrong/c:ext"}], "Particle": {"Kind": "Sequence", "Items": [{"Name": "x:CT_Wrong/c:ext", "Occurs": [{}], "InitialVersion": "Office2019"}]}},
    {"Name": "x:CT_Wrong/c:ext", "Version": "Office2019", "Children": [{"Name": "x:CT_Inner/x:inner"}]},
    {"Name": "x:CT_Inner/x:inner", "Version": "Office2019", "Attributes": [{"QName": "x:val", "Type": "BooleanValue"}]},
    {"Name": "c:CT_Other/c:ext", "Attributes": [{"QName": ":uri", "Type": "StringValue"}], "Particle": {"Kind": "Choice", "Items": [{"Name": "x:CT_Inner/x:inner", "InitialVersion": "Office2013"}, {"Kind": "Any", "Occurs": [{"Max": 1}]}]}}
  ]
}"#;

const ROW: &str = r#"
[[extension]]
list = "c:CT_List/c:extLst"
replaces = "x:CT_Wrong/c:ext"
type = "c:CT_Right/c:ext"
attributes_of = "c:CT_Other/c:ext"
children = ["x:CT_Inner/x:inner"]
"#;

const WORD_NAMESPACES: &str = r#"[
  {"Prefix": "", "Uri": ""},
  {"Prefix": "w", "Uri": "urn:w"}
]"#;

const WORD_TYPED_NAMESPACES: &str = r#"[{"Prefix": "w", "Namespace": "Office.Word"}]"#;

const WORD: &str = r#"{
  "TargetNamespace": "urn:w",
  "Types": [
    {"Name": "w:CT_Level/w:level", "Attributes": [{"QName": "w:val", "Type": "EnumValue<Office.Word.LevelValues>", "Validators": [{"Name": "RequiredValidator"}]}]},
    {"Name": "w:CT_OnOff/", "IsAbstract": true, "Attributes": [{"QName": "w:val", "Type": "OnOffValue"}]},
    {"Name": "w:CT_OnOff/w:bold", "IsDerived": true},
    {"Name": "w:CT_OnOffOnly/", "IsAbstract": true, "Attributes": [{"QName": "w:val", "Type": "EnumValue<Office.Word.OnOffOnlyValues>"}]},
    {"Name": "w:CT_OnOffOnly/w:bidi", "IsDerived": true},
    {"Name": "w:CT_Setting/w:setting", "Attributes": [{"QName": "w:name", "Type": "EnumValue<Office.Word.NameValues>", "Validators": [{"Name": "RequiredValidator"}]}]},
    {"Name": "w:CT_Sdt/w:sdt"},
    {"Name": "w:CT_XmlAttr/w:attr"},
    {"Name": "w:CT_Custom/w:custom", "Particle": {"Kind": "Sequence", "Items": [
      {"Name": "w:CT_XmlAttr/w:attr", "Occurs": [{"Max": 1}]},
      {"Kind": "Group", "Occurs": [{}], "Items": [{"Kind": "Choice", "Items": [{"Name": "w:CT_Sdt/w:sdt"}, {"Kind": "Group", "Items": [{"Kind": "Choice", "Items": [{"Name": "w:CT_Custom/w:custom"}]}]}]}]}
    ]}},
    {"Name": "w:CT_Para/w:para", "Particle": {"Kind": "Sequence", "Items": [
      {"Kind": "Group", "Occurs": [{}], "Items": [{"Kind": "Choice", "Items": [{"Name": "w:CT_Sdt/w:sdt"}, {"Kind": "Group", "Items": [{"Kind": "Choice", "Items": [{"Name": "w:CT_Custom/w:custom"}]}]}]}]}
    ]}},
    {"Name": "w:CT_Settings/w:settings", "Particle": {"Kind": "Sequence", "Items": [
      {"Name": "w:CT_OnOff/w:bold", "Occurs": [{"Max": 1}]},
      {"Name": "w:CT_Setting/w:setting", "Occurs": [{"Max": 1}]}
    ]}}
  ],
  "Enums": [
    {"Type": "w:ST_Jc", "Name": "JcValues", "Facets": [{"Value": "left"}, {"Value": "start"}]},
    {"Type": "w:ST_Level", "Name": "LevelValues", "Facets": [{"Value": "left"}]},
    {"Type": "w:ST_Only", "Name": "OnOffOnlyValues", "Facets": [{"Value": "on"}, {"Value": "off"}]},
    {"Type": "w:ST_Name", "Name": "NameValues", "Facets": [{"Value": "first"}]}
  ]
}"#;

const STANDARD: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns="urn:w" targetNamespace="urn:w" elementFormDefault="qualified" attributeFormDefault="qualified">
  <xsd:simpleType name="ST_String"><xsd:restriction base="xsd:string"/></xsd:simpleType>
  <xsd:group name="EG_Content"><xsd:choice><xsd:element name="para" type="CT_Para"/></xsd:choice></xsd:group>
  <xsd:complexType name="CT_Para"/>
  <xsd:complexType name="CT_Tag">
    <xsd:sequence>
      <xsd:element name="tagPr" type="CT_TagPr" minOccurs="0" maxOccurs="1"/>
      <xsd:group ref="EG_Content" minOccurs="0" maxOccurs="unbounded"/>
    </xsd:sequence>
    <xsd:attribute name="uri" type="ST_String"/>
    <xsd:attribute name="element" type="ST_String" use="required"/>
  </xsd:complexType>
  <xsd:complexType name="CT_TagPr">
    <xsd:sequence><xsd:element name="attr" type="CT_Attr" minOccurs="0" maxOccurs="unbounded"/></xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_TagType"><xsd:attribute name="url" type="ST_String"/></xsd:complexType>
  <xsd:complexType name="CT_Attr"/>
  <xsd:complexType name="CT_Settings">
    <xsd:sequence>
      <xsd:element name="bold" type="CT_Attr" minOccurs="0"/>
      <xsd:element name="tagType" type="CT_TagType" minOccurs="0" maxOccurs="unbounded"/>
      <xsd:element name="setting" type="CT_Attr" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
</xsd:schema>"#;

const ATTRIBUTE_TYPE_ROW: &str = r#"
[[attribute_type]]
type = "w:CT_Level/w:level"
attribute = "w:val"
enumeration = "w:ST_Jc"
"#;

const STRING_ROW: &str = r#"
[[attribute_type]]
type = "w:CT_Setting/w:setting"
attribute = "w:name"
string = true
"#;

const VALUES_ROW: &str = r#"
[[enumeration_values]]
enumeration = "w:ST_Level"
add = ["start", "end"]
"#;

const BASE_ROW: &str = r#"
[[base_type]]
type = "w:CT_OnOffOnly/w:bidi"
like = "w:CT_OnOff/w:bold"
"#;

const STANDARD_ROW: &str = r#"
[[from_standard]]
types = ["w:CT_Tag/w:tag", "w:CT_TagPr/w:tagPr", "w:CT_TagType/w:tagType"]
member = "w:CT_Tag/w:tag"
before = "w:CT_Sdt/w:sdt"
content_of = "w:CT_Custom/w:custom"
parents = ["w:CT_Settings/w:settings"]
"#;
fn read() -> SdkData {
    let system = MemorySystem::new()
        .with_file("data/namespaces.json", NAMESPACES.as_bytes())
        .unwrap()
        .with_file("data/typed/namespaces.json", b"[]")
        .unwrap()
        .with_file("data/schemas/chart.json", CHART.as_bytes())
        .unwrap()
        .with_file("data/parts/README.md", b"no parts")
        .unwrap()
        .with_file("data/schematrons.json", b"[]")
        .unwrap();
    SdkData::read(&system, Path::new("data")).unwrap()
}

fn read_word() -> SdkData {
    let system = MemorySystem::new()
        .with_file("data/namespaces.json", WORD_NAMESPACES.as_bytes())
        .unwrap()
        .with_file(
            "data/typed/namespaces.json",
            WORD_TYPED_NAMESPACES.as_bytes(),
        )
        .unwrap()
        .with_file("data/schemas/word.json", WORD.as_bytes())
        .unwrap()
        .with_file("data/parts/README.md", b"no parts")
        .unwrap()
        .with_file("data/schematrons.json", b"[]")
        .unwrap();
    SdkData::read(&system, Path::new("data")).unwrap()
}

fn standard() -> SchemaSet {
    let system = MemorySystem::new()
        .with_file("standard.xsd", STANDARD.as_bytes())
        .unwrap();
    SchemaSet::read(&system, &["standard.xsd"]).unwrap()
}

#[test]
fn a_row_replaces_the_type_and_points_the_list_at_the_new_one() {
    let mut data = read();
    data.correct(ROW, &SchemaSet::default()).unwrap();

    assert!(!data.types.contains_key("x:CT_Wrong/c:ext"));
    let list = &data.types["c:CT_List/c:extLst"];
    assert_eq!(list.children, ["c:CT_Right/c:ext"]);
    let item = &list.particle.as_ref().unwrap().items[0];
    assert_eq!(item.name.as_deref(), Some("c:CT_Right/c:ext"));
    assert_eq!(item.initial_version, None);

    let right = &data.types["c:CT_Right/c:ext"];
    assert_eq!(right.attributes, data.types["c:CT_Other/c:ext"].attributes);
    assert_eq!(right.children, ["x:CT_Inner/x:inner"]);
    assert_eq!(right.version, None);
    let items = &right.particle.as_ref().unwrap().items;
    assert_eq!(items[0].name.as_deref(), Some("x:CT_Inner/x:inner"));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].initial_version, None);
}

#[test]
fn a_row_naming_a_type_the_data_lacks_stops_the_generator() {
    for (from, to) in [
        ("x:CT_Wrong/c:ext", "x:CT_Missing/c:ext"),
        ("c:CT_List/c:extLst", "c:CT_Missing/c:extLst"),
        ("c:CT_Other/c:ext", "c:CT_Missing/c:ext"),
        ("x:CT_Inner/x:inner", "x:CT_Missing/x:inner"),
    ] {
        let mut data = read();
        let error = data
            .correct(&ROW.replace(from, to), &SchemaSet::default())
            .unwrap_err();
        assert!(format!("{error:#}").contains(to), "{error:#}");
    }
}

#[test]
fn a_row_whose_list_does_not_hold_the_type_stops_the_generator() {
    let mut data = read();
    let row = ROW.replace("c:CT_List/c:extLst", "c:CT_Other/c:ext");
    let error = data.correct(&row, &SchemaSet::default()).unwrap_err();
    assert!(format!("{error:#}").contains("does not hold"), "{error:#}");
}

#[test]
fn a_row_missing_a_field_stops_the_generator() {
    let mut data = read();
    let error = data
        .correct(
            "[[extension]]\nlist = \"c:CT_List/c:extLst\"",
            &SchemaSet::default(),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("has no"), "{error:#}");
}

#[test]
fn a_row_renames_the_attribute_it_names() {
    let mut data = read();
    data.correct(ATTRIBUTE_ROW, &SchemaSet::default()).unwrap();
    assert_eq!(data.types["x:CT_Inner/x:inner"].attributes[0].qname, ":val");
}

#[test]
fn an_attribute_row_naming_what_the_data_lacks_stops_the_generator() {
    for (from, to) in [
        ("x:CT_Inner/x:inner", "x:CT_Missing/x:inner"),
        ("x:val", "x:missing"),
    ] {
        let mut data = read();
        let error = data
            .correct(&ATTRIBUTE_ROW.replace(from, to), &SchemaSet::default())
            .unwrap_err();
        assert!(format!("{error:#}").contains(to), "{error:#}");
    }
}

#[test]
fn a_row_gives_an_attribute_the_enumeration_it_names() {
    let mut data = read_word();
    data.correct(ATTRIBUTE_TYPE_ROW, &standard()).unwrap();
    assert_eq!(
        data.types["w:CT_Level/w:level"].attributes[0].type_name,
        "EnumValue<Office.Word.JcValues>"
    );
}

#[test]
fn a_row_gives_an_attribute_the_string_class() {
    let mut data = read_word();
    data.correct(STRING_ROW, &standard()).unwrap();
    let attribute = &data.types["w:CT_Setting/w:setting"].attributes[0];
    assert_eq!(attribute.type_name, "StringValue");
    assert_eq!(attribute.validators.len(), 1);
}

#[test]
fn an_attribute_type_row_that_names_what_the_data_lacks_or_names_no_class_stops_the_generator() {
    for (from, to) in [
        ("w:CT_Level/w:level", "w:CT_Missing/w:level"),
        ("w:val", "w:missing"),
        ("w:ST_Jc", "w:ST_Missing"),
    ] {
        let mut data = read_word();
        let error = data
            .correct(&ATTRIBUTE_TYPE_ROW.replace(from, to), &standard())
            .unwrap_err();
        assert!(format!("{error:#}").contains(to), "{error:#}");
    }
    let mut data = read_word();
    let row = ATTRIBUTE_TYPE_ROW.replace("enumeration = \"w:ST_Jc\"", "");
    let error = data.correct(&row, &standard()).unwrap_err();
    assert!(format!("{error:#}").contains("exactly one"), "{error:#}");
}

#[test]
fn a_row_adds_values_to_an_enumeration() {
    let mut data = read_word();
    data.correct(VALUES_ROW, &standard()).unwrap();
    let values: Vec<&str> = data.enumerations["w:ST_Level"]
        .facets
        .iter()
        .map(|facet| facet.value.as_str())
        .collect();
    assert_eq!(values, ["left", "start", "end"]);
}

#[test]
fn an_enumeration_values_row_that_names_what_the_data_lacks_or_repeats_a_value_stops_the_generator()
{
    let mut data = read_word();
    let error = data
        .correct(
            &VALUES_ROW.replace("w:ST_Level", "w:ST_Missing"),
            &standard(),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("w:ST_Missing"), "{error:#}");
    let repeated = data
        .correct(&VALUES_ROW.replace("\"end\"", "\"left\""), &standard())
        .unwrap_err();
    assert!(format!("{repeated:#}").contains("already"), "{repeated:#}");
}

#[test]
fn a_row_gives_a_derived_type_the_attributes_of_the_type_it_is_like() {
    let mut data = read_word();
    data.correct(BASE_ROW, &standard()).unwrap();
    let bidi = &data.types["w:CT_OnOffOnly/w:bidi"];
    assert!(!bidi.is_derived);
    assert_eq!(bidi.attributes, data.types["w:CT_OnOff/"].attributes);
}

#[test]
fn a_base_type_row_that_names_what_the_data_lacks_stops_the_generator() {
    for (from, to) in [
        ("w:CT_OnOffOnly/w:bidi", "w:CT_Missing/w:bidi"),
        ("w:CT_OnOff/w:bold", "w:CT_Missing/w:bold"),
    ] {
        let mut data = read_word();
        let error = data
            .correct(&BASE_ROW.replace(from, to), &standard())
            .unwrap_err();
        assert!(format!("{error:#}").contains(to), "{error:#}");
    }
}

#[test]
fn a_row_builds_the_types_the_standard_declares_and_places_them() {
    let mut data = read_word();
    data.correct(STANDARD_ROW, &standard()).unwrap();

    let tag = &data.types["w:CT_Tag/w:tag"];
    let qnames: Vec<&str> = tag.attributes.iter().map(|a| a.qname.as_str()).collect();
    assert_eq!(qnames, ["w:uri", "w:element"]);
    assert_eq!(tag.attributes[0].validators.len(), 0);
    assert_eq!(tag.attributes[1].validators[0].name, "RequiredValidator");
    let items = &tag.particle.as_ref().unwrap().items;
    assert_eq!(items[0].name.as_deref(), Some("w:CT_TagPr/w:tagPr"));
    assert_eq!(items[0].occurs[0].max, 1);
    let group = &items[1];
    assert_eq!(group.kind.as_deref(), Some("Group"));
    let choice = &group.items[0];
    assert_eq!(choice.items[0].name.as_deref(), Some("w:CT_Tag/w:tag"));
    assert_eq!(choice.items[1].name.as_deref(), Some("w:CT_Sdt/w:sdt"));

    let pr = &data.types["w:CT_TagPr/w:tagPr"];
    let attr = &pr.particle.as_ref().unwrap().items[0];
    assert_eq!(attr.name.as_deref(), Some("w:CT_XmlAttr/w:attr"));
    assert_eq!(attr.occurs[0].max, 0);

    assert_eq!(
        data.types["w:CT_TagType/w:tagType"].attributes[0].qname,
        "w:url"
    );

    for holder in ["w:CT_Para/w:para", "w:CT_Custom/w:custom"] {
        let particle = data.types[holder].particle.as_ref().unwrap();
        let content = particle.items.last().unwrap();
        let names: Vec<_> = content.items[0]
            .items
            .iter()
            .map(|i| i.name.as_deref())
            .collect();
        assert_eq!(
            names[..2],
            [Some("w:CT_Tag/w:tag"), Some("w:CT_Sdt/w:sdt")],
            "{holder}"
        );
    }

    let settings = &data.types["w:CT_Settings/w:settings"]
        .particle
        .as_ref()
        .unwrap()
        .items;
    let names: Vec<_> = settings
        .iter()
        .map(|i| i.name.as_deref().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "w:CT_OnOff/w:bold",
            "w:CT_TagType/w:tagType",
            "w:CT_Setting/w:setting"
        ]
    );
}

#[test]
fn a_from_standard_row_that_names_what_the_data_or_the_standard_lacks_stops_the_generator() {
    for (from, to) in [
        (
            "before = \"w:CT_Sdt/w:sdt\"",
            "before = \"w:CT_Missing/w:sdt\"",
        ),
        (
            "content_of = \"w:CT_Custom/w:custom\"",
            "content_of = \"w:CT_Missing/w:custom\"",
        ),
        (
            "parents = [\"w:CT_Settings/w:settings\"]",
            "parents = [\"w:CT_Missing/w:settings\"]",
        ),
        (
            "member = \"w:CT_Tag/w:tag\"",
            "member = \"w:CT_Other/w:other\"",
        ),
        ("\"w:CT_TagType/w:tagType\"", "\"w:CT_Absent/w:absent\""),
    ] {
        let mut data = read_word();
        assert!(
            data.correct(&STANDARD_ROW.replace(from, to), &standard())
                .is_err(),
            "{to}"
        );
    }
}
