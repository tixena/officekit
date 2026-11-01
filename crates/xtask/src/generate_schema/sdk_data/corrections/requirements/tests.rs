use std::path::Path;

use os_shim::mock::MemorySystem;

use crate::generate_schema::model::SchemaSet;
use crate::generate_schema::sdk_data::{DataParticle, SdkData};

const NAMESPACES: &str = r#"[
  {"Prefix": "", "Uri": ""},
  {"Prefix": "w", "Uri": "urn:w"},
  {"Prefix": "w15", "Uri": "urn:w15"}
]"#;

const TYPED_NAMESPACES: &str = r#"[{"Prefix": "w", "Namespace": "Office.Word"}]"#;

/// The data: attributes the standard requires and the data leaves optional, a choice where the
/// standard has a sequence, and a child the standard requires and the data does not.
const DATA: &str = r#"{
  "TargetNamespace": "urn:w",
  "Types": [
    {"Name": "w:CT_Margin/w:margin", "Attributes": [
      {"QName": "w:top", "Type": "StringValue"},
      {"QName": "w:left", "Type": "StringValue"},
      {"QName": "w:gutter", "Type": "StringValue"}
    ]},
    {"Name": "w:CT_Cnf/w:cnf", "Attributes": [{"QName": "w:val", "Type": "StringValue"}]},
    {"Name": "w:CT_OnOff/", "IsAbstract": true, "Attributes": [{"QName": "w:val", "Type": "StringValue"}]},
    {"Name": "w:CT_OnOff/w:flag", "IsDerived": true},
    {"Name": "w:CT_A/w:alias"},
    {"Name": "w:CT_A/w:temp"},
    {"Name": "w:CT_A/w:kind"},
    {"Name": "w:CT_A/w:data"},
    {"Name": "w:CT_Color/w15:color", "Version": "Office2013"},
    {"Name": "w:CT_Props/w:props", "Particle": {"Kind": "Choice", "Occurs": [{}], "Items": [
      {"Name": "w:CT_A/w:alias"},
      {"Name": "w:CT_A/w:temp"},
      {"Name": "w:CT_Color/w15:color", "InitialVersion": "Office2013"}
    ]}},
    {"Name": "w:CT_Merge/w:merge", "Particle": {"Kind": "Sequence", "Items": [
      {"Name": "w:CT_A/w:kind", "Occurs": [{"Max": 1}]},
      {"Name": "w:CT_A/w:data"}
    ]}},
    {"Name": "w:CT_Free/w:free", "Particle": {"Kind": "Choice", "Occurs": [{}], "Items": [
      {"Name": "w:CT_A/w:alias"},
      {"Name": "w:CT_A/w:temp"}
    ]}},
    {"Name": "w:CT_Custom/w:custom"}
  ]
}"#;

/// The standard: what it requires of each type above.
const STANDARD: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns="urn:w" targetNamespace="urn:w" elementFormDefault="qualified" attributeFormDefault="qualified">
  <xsd:complexType name="CT_Margin">
    <xsd:attribute name="top" type="xsd:string" use="required"/>
    <xsd:attribute name="left" type="xsd:string"/>
    <xsd:attribute name="gutter" type="xsd:string" use="required"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Cnf">
    <xsd:attribute name="val" type="xsd:string" use="required"/>
  </xsd:complexType>
  <xsd:complexType name="CT_OnOff">
    <xsd:attribute name="val" type="xsd:string" use="required"/>
  </xsd:complexType>
  <xsd:complexType name="CT_A"/>
  <xsd:complexType name="CT_Props">
    <xsd:sequence>
      <xsd:element name="alias" type="CT_A" minOccurs="0"/>
      <xsd:element name="temp" type="CT_A" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Merge">
    <xsd:sequence>
      <xsd:element name="kind" type="CT_A"/>
      <xsd:element name="data" type="CT_A"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Free">
    <xsd:sequence>
      <xsd:element name="alias" type="CT_A" minOccurs="0"/>
      <xsd:element name="temp" type="CT_A" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Custom">
    <xsd:attribute name="element" type="xsd:string" use="required"/>
  </xsd:complexType>
</xsd:schema>"#;

/// The rows that keep the data's form: an optional attribute, a free order, and an attribute the
/// data does not declare.
const TABLE: &str = r#"
[[optional]]
type = "w:CT_Cnf/w:cnf"
attribute = "w:val"

[[any_order]]
type = "w:CT_Free/w:free"

[[undeclared_required]]
type = "w:CT_Custom/w:custom"
attribute = "w:element"
"#;

/// The data with what the standard requires, under the rows of `table`.
fn required(table: &str) -> anyhow::Result<SdkData> {
    let system = MemorySystem::new()
        .with_file("/data/namespaces.json", NAMESPACES.as_bytes())?
        .with_file("/data/typed/namespaces.json", TYPED_NAMESPACES.as_bytes())?
        .with_file("/data/schemas/word.json", DATA.as_bytes())?
        .with_file("/data/parts/README.md", b"no parts")?
        .with_file("/data/schematrons.json", b"[]")?
        .with_file("/standard.xsd", STANDARD.as_bytes())?;
    let mut data = SdkData::read(&system, Path::new("data"))?;
    let standard = SchemaSet::read(&system, &["standard.xsd"])?;
    data.require_standard(table, &standard)?;
    Ok(data)
}

/// Whether the attribute `qname` of the type `name` holds the required check.
fn is_required(data: &SdkData, name: &str, qname: &str) -> bool {
    data.types[name]
        .attributes
        .iter()
        .find(|attribute| attribute.qname == qname)
        .unwrap()
        .validators
        .iter()
        .any(|validator| validator.name == "RequiredValidator")
}

/// The names of a particle's items, `None` for a composite.
fn item_names(particle: &DataParticle) -> Vec<Option<&str>> {
    particle
        .items
        .iter()
        .map(|item| item.name.as_deref())
        .collect()
}

// XSD and SDK data input: the generator reads them, forms the typed model does not hold.
#[test]
fn each_attribute_the_standard_requires_takes_the_required_check() {
    let data = required(TABLE).unwrap();
    assert!(is_required(&data, "w:CT_Margin/w:margin", "w:top"));
    assert!(is_required(&data, "w:CT_Margin/w:margin", "w:gutter"));
    assert!(!is_required(&data, "w:CT_Margin/w:margin", "w:left"));
    assert!(
        is_required(&data, "w:CT_OnOff/", "w:val"),
        "a derived type's base holds it"
    );
    assert!(
        !is_required(&data, "w:CT_Cnf/w:cnf", "w:val"),
        "an optional row keeps it optional"
    );
}

// XSD and SDK data input: the generator reads them, forms the typed model does not hold.
#[test]
fn a_choice_where_the_standard_has_a_sequence_takes_the_sequence_with_extensions_anywhere() {
    let data = required(TABLE).unwrap();
    let props = data.types["w:CT_Props/w:props"].particle.as_ref().unwrap();
    assert_eq!(props.kind.as_deref(), Some("Sequence"));
    assert_eq!(
        item_names(props),
        [
            None,
            Some("w:CT_A/w:alias"),
            None,
            Some("w:CT_A/w:temp"),
            None
        ]
    );
    let extensions = &props.items[0];
    assert_eq!(extensions.kind.as_deref(), Some("Choice"));
    assert_eq!(item_names(extensions), [Some("w:CT_Color/w15:color")]);
    assert_eq!(extensions.occurs[0].min, 0);
    let free = data.types["w:CT_Free/w:free"].particle.as_ref().unwrap();
    assert_eq!(
        free.kind.as_deref(),
        Some("Choice"),
        "an any_order row keeps it"
    );
}

// XSD and SDK data input: the generator reads them, forms the typed model does not hold.
#[test]
fn a_child_the_standard_requires_is_required() {
    let data = required(TABLE).unwrap();
    let merge = data.types["w:CT_Merge/w:merge"].particle.as_ref().unwrap();
    assert_eq!(merge.items[0].name.as_deref(), Some("w:CT_A/w:kind"));
    assert!(merge.items[0].occurs.is_empty(), "exactly once");
}

// XSD and SDK data input: the generator reads them, forms the typed model does not hold.
#[test]
fn an_attribute_the_data_does_not_declare_stops_the_build_unless_a_row_names_it() {
    let unnamed = required(&TABLE.replace(
        "[[undeclared_required]]\ntype = \"w:CT_Custom/w:custom\"\nattribute = \"w:element\"\n",
        "",
    ))
    .unwrap_err()
    .to_string();
    assert!(
        unnamed.contains("w:element of w:CT_Custom/w:custom"),
        "{unnamed}"
    );
    let stale = required(&format!(
        "{TABLE}\n[[undeclared_required]]\ntype = \"w:CT_Margin/w:margin\"\nattribute = \"w:top\"\n"
    ))
    .unwrap_err()
    .to_string();
    assert!(stale.contains("which the data declares"), "{stale}");
}
