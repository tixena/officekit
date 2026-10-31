use os_shim::mock::MemorySystem;

use super::{AttributeRule, ChildRule, Rules, build, spellings_name, static_name};
use crate::generate_schema::emit::classes::correspondences::{Correspondences, Spelling};
use crate::generate_schema::emit::namespaces::Row;
use crate::generate_schema::model::SchemaSet;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const X_STRICT: &str = "http://purl.oclc.org/ooxml/spreadsheetml/main";
const V: &str = "urn:schemas-microsoft-com:vml";

/// Transitional `wml.xsd`: a value Strict spells otherwise, a member Strict
/// lacks, a renamed child and attribute, a refused child, and one tag with
/// two types under two parents.
const TRANSITIONAL_WML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  targetNamespace="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  elementFormDefault="qualified" attributeFormDefault="qualified">
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
  <xsd:complexType name="CT_Jc">
    <xsd:attribute name="val" type="ST_Jc"/>
  </xsd:complexType>
  <xsd:complexType name="CT_OnOff">
    <xsd:attribute name="val" type="ST_OnOff"/>
  </xsd:complexType>
  <xsd:complexType name="CT_DecimalNumber">
    <xsd:attribute name="val" type="xsd:integer"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Ind">
    <xsd:attribute name="start" type="xsd:integer"/>
    <xsd:attribute name="left" type="xsd:integer"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Border"/>
  <xsd:complexType name="CT_TblBorders">
    <xsd:sequence>
      <xsd:element name="top" type="CT_Border" minOccurs="0"/>
      <xsd:element name="start" type="CT_Border" minOccurs="0"/>
      <xsd:element name="end" type="CT_Border" minOccurs="0"/>
      <xsd:element name="left" type="CT_Border" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Compat">
    <xsd:sequence>
      <xsd:element name="useFELayout" type="CT_OnOff" minOccurs="0"/>
      <xsd:element name="spaceForUL" type="CT_OnOff" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_CheckBox">
    <xsd:sequence>
      <xsd:element name="default" type="CT_OnOff" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_DdList">
    <xsd:sequence>
      <xsd:element name="default" type="CT_DecimalNumber" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_PPr">
    <xsd:sequence>
      <xsd:element name="jc" type="CT_Jc" minOccurs="0"/>
      <xsd:element name="ind" type="CT_Ind" minOccurs="0"/>
      <xsd:element name="tblBorders" type="CT_TblBorders" minOccurs="0"/>
      <xsd:element name="compat" type="CT_Compat" minOccurs="0"/>
      <xsd:element name="checkBox" type="CT_CheckBox" minOccurs="0"/>
      <xsd:element name="ddList" type="CT_DdList" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:element name="pPr" type="CT_PPr"/>
</xsd:schema>"#;

/// Strict `wml.xsd`: the same definitions without the Transitional additions.
const STRICT_WML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://purl.oclc.org/ooxml/wordprocessingml/main"
  targetNamespace="http://purl.oclc.org/ooxml/wordprocessingml/main"
  elementFormDefault="qualified" attributeFormDefault="qualified">
  <xsd:simpleType name="ST_Jc">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="start"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_OnOff">
    <xsd:union memberTypes="xsd:boolean"/>
  </xsd:simpleType>
  <xsd:complexType name="CT_Jc">
    <xsd:attribute name="val" type="ST_Jc"/>
  </xsd:complexType>
  <xsd:complexType name="CT_OnOff">
    <xsd:attribute name="val" type="ST_OnOff"/>
  </xsd:complexType>
  <xsd:complexType name="CT_DecimalNumber">
    <xsd:attribute name="val" type="xsd:integer"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Ind">
    <xsd:attribute name="start" type="xsd:integer"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Border"/>
  <xsd:complexType name="CT_TblBorders">
    <xsd:sequence>
      <xsd:element name="top" type="CT_Border" minOccurs="0"/>
      <xsd:element name="start" type="CT_Border" minOccurs="0"/>
      <xsd:element name="end" type="CT_Border" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Compat">
    <xsd:sequence>
      <xsd:element name="spaceForUL" type="CT_OnOff" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_CheckBox">
    <xsd:sequence>
      <xsd:element name="default" type="CT_OnOff" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_DdList">
    <xsd:sequence>
      <xsd:element name="default" type="CT_DecimalNumber" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_PPr">
    <xsd:sequence>
      <xsd:element name="jc" type="CT_Jc" minOccurs="0"/>
      <xsd:element name="ind" type="CT_Ind" minOccurs="0"/>
      <xsd:element name="tblBorders" type="CT_TblBorders" minOccurs="0"/>
      <xsd:element name="compat" type="CT_Compat" minOccurs="0"/>
      <xsd:element name="checkBox" type="CT_CheckBox" minOccurs="0"/>
      <xsd:element name="ddList" type="CT_DdList" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:element name="pPr" type="CT_PPr"/>
</xsd:schema>"#;

/// Transitional `sml.xsd`: a cell type without `d`.
const TRANSITIONAL_SML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
  targetNamespace="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <xsd:simpleType name="ST_CellType">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="n"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:complexType name="CT_Cell">
    <xsd:attribute name="t" type="ST_CellType"/>
  </xsd:complexType>
  <xsd:element name="c" type="CT_Cell"/>
</xsd:schema>"#;

/// Strict `sml.xsd`: the cell type with `d`.
const STRICT_SML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://purl.oclc.org/ooxml/spreadsheetml/main"
  targetNamespace="http://purl.oclc.org/ooxml/spreadsheetml/main">
  <xsd:simpleType name="ST_CellType">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="n"/>
      <xsd:enumeration value="d"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:complexType name="CT_Cell">
    <xsd:attribute name="t" type="ST_CellType"/>
  </xsd:complexType>
  <xsd:element name="c" type="CT_Cell"/>
</xsd:schema>"#;

/// A Transitional file with no Strict twin.
const VML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  targetNamespace="urn:schemas-microsoft-com:vml">
  <xsd:complexType name="CT_Shape"/>
  <xsd:element name="shape" type="CT_Shape"/>
</xsd:schema>"#;

/// The correspondences of the fixture's additions.
const TABLE: &str = r#"
[[value]]
type = "w:ST_Jc"
values = { left = "start" }

[[member]]
type = "w:ST_OnOff"
member = "w:ST_OnOff1"
values = { on = "true", off = "false" }

[[child]]
type = "w:CT_TblBorders"
renames = { left = "start" }

[[attribute]]
type = "w:CT_Ind"
renames = { left = "start" }
"#;

fn row(prefix: &str, uri: &str, strict: Option<&str>) -> Row {
    Row {
        introduced: None,
        prefix: prefix.to_owned(),
        strict_uri: strict.map(ToOwned::to_owned),
        uri: uri.to_owned(),
    }
}

/// The fixture's rules: `v` is namespace 0, `w` 1, `x` 2.
fn rules(table: &str) -> anyhow::Result<Rules> {
    let system = MemorySystem::new()
        .with_file("/transitional/sml.xsd", TRANSITIONAL_SML.as_bytes())?
        .with_file("/transitional/vml-main.xsd", VML.as_bytes())?
        .with_file("/transitional/wml.xsd", TRANSITIONAL_WML.as_bytes())?
        .with_file("/strict/sml.xsd", STRICT_SML.as_bytes())?
        .with_file("/strict/wml.xsd", STRICT_WML.as_bytes())?;
    let transitional = SchemaSet::read(
        &system,
        &[
            "transitional/sml.xsd",
            "transitional/vml-main.xsd",
            "transitional/wml.xsd",
        ],
    )?;
    let strict = SchemaSet::read(&system, &["strict/sml.xsd", "strict/wml.xsd"])?;
    let rows = [
        row("v", V, None),
        row("w", W, Some(W_STRICT)),
        row("x", X, Some(X_STRICT)),
    ];
    let correspondences = Correspondences::parse(table, &rows)?;
    build(&transitional, &strict, &rows, &correspondences)
}

fn tag(namespace: Option<u8>, local: &str) -> (Option<u8>, String) {
    (namespace, local.to_owned())
}

#[test]
fn every_addition_takes_its_rule() {
    let found = rules(TABLE).unwrap();
    let on_off = vec![Spelling::Map(vec![
        ("off".to_owned(), "false".to_owned()),
        ("on".to_owned(), "true".to_owned()),
    ])];
    assert_eq!(
        found
            .attributes
            .get(&(tag(Some(1), "jc"), tag(Some(1), "val"))),
        Some(&AttributeRule::Values(vec![Spelling::Map(vec![(
            "left".to_owned(),
            "start".to_owned()
        )])]))
    );
    assert_eq!(
        found
            .attributes
            .get(&(tag(Some(1), "ind"), tag(Some(1), "left"))),
        Some(&AttributeRule::Rename("start".to_owned()))
    );
    assert_eq!(
        found
            .attributes
            .get(&(tag(Some(1), "spaceForUL"), tag(Some(1), "val"))),
        Some(&AttributeRule::Values(on_off.clone()))
    );
    assert_eq!(
        found
            .children
            .get(&(tag(Some(1), "tblBorders"), tag(Some(1), "left"))),
        Some(&ChildRule::Rename {
            before: vec!["end".to_owned()],
            to: "start".to_owned(),
        })
    );
    assert_eq!(
        found
            .children
            .get(&(tag(Some(1), "compat"), tag(Some(1), "useFELayout"))),
        Some(&ChildRule::Refuse("useFELayout".to_owned()))
    );
    assert!(found.elements.contains(&tag(Some(1), "useFELayout")));
    assert_eq!(found.namespaces.get(&0), Some(&vec!["shape".to_owned()]));
    assert_eq!(
        found.spellings.get(&on_off).map(String::as_str),
        Some("W_ST_ON_OFF")
    );
    assert_eq!(
        found.strict_only.get(&(tag(Some(2), "c"), tag(None, "t"))),
        Some(&(vec!["d".to_owned()], "c/@t".to_owned()))
    );
}

#[test]
fn a_tag_whose_types_disagree_takes_its_rule_by_parent() {
    let found = rules(TABLE).unwrap();
    let (owner, attribute) = (tag(Some(1), "default"), tag(Some(1), "val"));
    assert_eq!(
        found.attributes.get(&(owner.clone(), attribute.clone())),
        None
    );
    assert!(found.contextual.contains_key(&(
        tag(Some(1), "checkBox"),
        owner.clone(),
        attribute.clone()
    )));
    assert!(
        !found
            .contextual
            .contains_key(&(tag(Some(1), "ddList"), owner, attribute))
    );
}

#[test]
fn an_addition_with_no_row_or_a_row_with_no_addition_stops_the_build() {
    let missing = rules(&TABLE.replace(
        "[[value]]\ntype = \"w:ST_Jc\"\nvalues = { left = \"start\" }\n",
        "",
    ))
    .unwrap_err()
    .to_string();
    assert!(missing.contains("has no `value` row"), "{missing}");
    let stale = rules(&format!(
        "{TABLE}\n[[attribute]]\ntype = \"w:CT_Ind\"\nrenames = {{ right = \"end\" }}\n"
    ))
    .unwrap_err()
    .to_string();
    assert!(stale.contains("no element of that type adds"), "{stale}");
}

#[test]
fn a_static_is_named_after_its_type_or_its_unit() {
    assert_eq!(static_name("s", "ST_OnOff"), "S_ST_ON_OFF");
    assert_eq!(
        static_name("a", "ST_TextFontScalePercentOrPercentString"),
        "A_ST_TEXT_FONT_SCALE_PERCENT_OR_PERCENT_STRING"
    );
    assert_eq!(
        spellings_name(&[Spelling::Percent {
            decimals: Some(2),
            per_percent: 1000,
        }])
        .as_deref(),
        Some("PERCENT_IN_1000THS_TO_2_PLACES")
    );
    assert_eq!(
        spellings_name(&[Spelling::Percent {
            decimals: None,
            per_percent: 1,
        }])
        .as_deref(),
        Some("WHOLE_PERCENT")
    );
    assert_eq!(spellings_name(&[Spelling::Map(Vec::new())]), None);
}
