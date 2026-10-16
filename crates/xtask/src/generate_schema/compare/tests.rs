use alloc::collections::BTreeSet;

use os_shim::mock::MemorySystem;

use super::{Additions, Wildcard, strict_only, transitional_only};
use crate::generate_schema::model::{Name, SchemaSet, XSD_NAMESPACE};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

/// Transitional `wml.xsd`: every kind of addition the comparison reports.
const TRANSITIONAL_WML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  targetNamespace="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  elementFormDefault="qualified" attributeFormDefault="qualified">
  <xsd:complexType name="CT_OnOff"/>
  <xsd:complexType name="CT_P"/>
  <xsd:complexType name="CT_SectPr"/>
  <xsd:complexType name="CT_Drawing"/>
  <xsd:complexType name="CT_TxbxContent">
    <xsd:sequence>
      <xsd:element name="p" type="CT_P"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:group name="EG_BlockLevelElts">
    <xsd:choice>
      <xsd:element name="p" type="CT_P"/>
    </xsd:choice>
  </xsd:group>
  <xsd:complexType name="CT_Body">
    <xsd:sequence>
      <xsd:group ref="EG_BlockLevelElts" minOccurs="0" maxOccurs="unbounded"/>
      <xsd:element name="sectPr" type="CT_SectPr" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Compat">
    <xsd:sequence>
      <xsd:element name="useFELayout" type="CT_OnOff" minOccurs="0"/>
      <xsd:element name="spaceForUL" type="CT_OnOff" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:attributeGroup name="AG_Password">
    <xsd:attribute name="hashValue" type="xsd:base64Binary"/>
  </xsd:attributeGroup>
  <xsd:attributeGroup name="AG_TransitionalPassword">
    <xsd:attribute name="hash" type="xsd:base64Binary"/>
    <xsd:attribute name="salt" type="xsd:base64Binary"/>
  </xsd:attributeGroup>
  <xsd:complexType name="CT_DocProtect">
    <xsd:attribute name="edit" type="xsd:string"/>
    <xsd:attributeGroup ref="AG_Password"/>
    <xsd:attributeGroup ref="AG_TransitionalPassword"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Ind">
    <xsd:attribute name="start" type="xsd:integer"/>
    <xsd:attribute name="left" type="xsd:integer"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Object">
    <xsd:sequence>
      <xsd:any processContents="lax" namespace="urn:schemas-microsoft-com:vml"
        minOccurs="0" maxOccurs="unbounded"/>
      <xsd:element name="drawing" type="CT_Drawing" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:simpleType name="ST_Jc">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="start"/>
      <xsd:enumeration value="left"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_OnOff1">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="on"/>
      <xsd:enumeration value="off"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_OnOff">
    <xsd:union memberTypes="xsd:boolean ST_OnOff1"/>
  </xsd:simpleType>
</xsd:schema>"#;

/// Strict `wml.xsd`: the same definitions without the Transitional additions.
const STRICT_WML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://purl.oclc.org/ooxml/wordprocessingml/main"
  targetNamespace="http://purl.oclc.org/ooxml/wordprocessingml/main"
  elementFormDefault="qualified" attributeFormDefault="qualified">
  <xsd:complexType name="CT_OnOff"/>
  <xsd:complexType name="CT_P"/>
  <xsd:complexType name="CT_SectPr"/>
  <xsd:complexType name="CT_Drawing"/>
  <xsd:group name="EG_BlockLevelElts">
    <xsd:choice>
      <xsd:element name="p" type="CT_P"/>
    </xsd:choice>
  </xsd:group>
  <xsd:complexType name="CT_Body">
    <xsd:sequence>
      <xsd:group ref="EG_BlockLevelElts" minOccurs="0" maxOccurs="unbounded"/>
      <xsd:element name="sectPr" type="CT_SectPr" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Compat">
    <xsd:sequence>
      <xsd:element name="spaceForUL" type="CT_OnOff" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:attributeGroup name="AG_Password">
    <xsd:attribute name="hashValue" type="xsd:base64Binary"/>
  </xsd:attributeGroup>
  <xsd:complexType name="CT_DocProtect">
    <xsd:attribute name="edit" type="xsd:string"/>
    <xsd:attributeGroup ref="AG_Password"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Ind">
    <xsd:attribute name="start" type="xsd:integer"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Object">
    <xsd:sequence>
      <xsd:element name="drawing" type="CT_Drawing" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:simpleType name="ST_Jc">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="start"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_OnOff">
    <xsd:union memberTypes="xsd:boolean"/>
  </xsd:simpleType>
</xsd:schema>"#;

/// Transitional `sml.xsd`: a cell type without `d`.
const TRANSITIONAL_SML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  targetNamespace="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <xsd:simpleType name="ST_CellType">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="b"/>
      <xsd:enumeration value="n"/>
    </xsd:restriction>
  </xsd:simpleType>
</xsd:schema>"#;

/// Strict `sml.xsd`: the cell type with `d`.
const STRICT_SML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  targetNamespace="http://purl.oclc.org/ooxml/spreadsheetml/main">
  <xsd:simpleType name="ST_CellType">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="b"/>
      <xsd:enumeration value="n"/>
      <xsd:enumeration value="d"/>
    </xsd:restriction>
  </xsd:simpleType>
</xsd:schema>"#;

/// A Transitional file with no Strict twin, which the comparison leaves out.
const VML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  targetNamespace="urn:schemas-microsoft-com:vml">
  <xsd:complexType name="CT_Shape"/>
  <xsd:simpleType name="ST_Ext">
    <xsd:restriction base="xsd:string"/>
  </xsd:simpleType>
</xsd:schema>"#;

fn name(namespace: &str, local: &str) -> Name {
    Name::new(namespace, local)
}

fn sets() -> (SchemaSet, SchemaSet) {
    let system = MemorySystem::new()
        .with_file("transitional/sml.xsd", TRANSITIONAL_SML.as_bytes())
        .unwrap()
        .with_file("transitional/vml-main.xsd", VML.as_bytes())
        .unwrap()
        .with_file("transitional/wml.xsd", TRANSITIONAL_WML.as_bytes())
        .unwrap()
        .with_file("strict/sml.xsd", STRICT_SML.as_bytes())
        .unwrap()
        .with_file("strict/wml.xsd", STRICT_WML.as_bytes())
        .unwrap();
    let transitional = SchemaSet::read(
        &system,
        &[
            "transitional/sml.xsd",
            "transitional/vml-main.xsd",
            "transitional/wml.xsd",
        ],
    )
    .unwrap();
    let strict = SchemaSet::read(&system, &["strict/sml.xsd", "strict/wml.xsd"]).unwrap();
    (transitional, strict)
}

fn wordprocessing(local: &str) -> Name {
    name(W, local)
}

#[test]
fn reports_every_kind_of_transitional_addition() {
    let (transitional, strict) = sets();
    assert_eq!(
        transitional_only(&transitional, &strict),
        Additions {
            attribute_groups: BTreeSet::from([wordprocessing("AG_TransitionalPassword")]),
            attributes: BTreeSet::from([(wordprocessing("CT_Ind"), wordprocessing("left"))]),
            complex_types: BTreeSet::from([wordprocessing("CT_TxbxContent")]),
            elements: BTreeSet::from([(
                wordprocessing("CT_Compat"),
                wordprocessing("useFELayout")
            )]),
            enumeration_values: BTreeSet::from([(wordprocessing("ST_Jc"), "left".to_owned())]),
            group_references: BTreeSet::from([(
                wordprocessing("CT_DocProtect"),
                wordprocessing("AG_TransitionalPassword"),
            )]),
            groups: BTreeSet::new(),
            simple_types: BTreeSet::from([wordprocessing("ST_OnOff1")]),
            union_members: BTreeSet::from([(
                wordprocessing("ST_OnOff"),
                wordprocessing("ST_OnOff1")
            )]),
            wildcards: BTreeSet::from([(
                wordprocessing("CT_Object"),
                Wildcard {
                    namespace: "urn:schemas-microsoft-com:vml".to_owned(),
                    process_contents: "lax".to_owned(),
                },
            )]),
        }
    );
}

#[test]
fn reports_what_strict_adds_in_the_transitional_namespaces() {
    let (transitional, strict) = sets();
    let only = strict_only(&transitional, &strict);
    assert_eq!(
        only.enumeration_values,
        BTreeSet::from([(name(X, "ST_CellType"), "d".to_owned())])
    );
    assert_eq!(
        Additions {
            enumeration_values: BTreeSet::new(),
            ..only
        },
        Additions::default()
    );
}

#[test]
fn counts_the_shared_definitions_that_gain_children_attributes_references_or_values() {
    let (transitional, strict) = sets();
    let only = transitional_only(&transitional, &strict);
    assert_eq!(
        only.extended_definitions(),
        BTreeSet::from([
            &wordprocessing("CT_Compat"),
            &wordprocessing("CT_DocProtect"),
            &wordprocessing("CT_Ind"),
            &wordprocessing("ST_Jc"),
        ])
    );
}

#[test]
fn leaves_out_the_files_without_a_twin() {
    let (transitional, strict) = sets();
    let only = transitional_only(&transitional, &strict);
    let vml = "urn:schemas-microsoft-com:vml";
    assert!(!only.complex_types.contains(&name(vml, "CT_Shape")));
    assert!(!only.simple_types.contains(&name(vml, "ST_Ext")));
}

#[test]
fn finds_nothing_between_a_set_and_itself() {
    let (transitional, _) = sets();
    assert!(transitional_only(&transitional, &transitional).is_empty());
    assert!(strict_only(&transitional, &transitional).is_empty());
    assert!(
        !Additions {
            groups: BTreeSet::from([name(XSD_NAMESPACE, "any")]),
            ..Additions::default()
        }
        .is_empty()
    );
}
