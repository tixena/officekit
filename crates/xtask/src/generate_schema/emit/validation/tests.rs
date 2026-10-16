use alloc::collections::BTreeMap;
use std::path::Path;

use os_shim::mock::MemorySystem;

use super::{
    AttributeRow, Bounds, Check, ChildRow, CompositeKind, Marks, PartRow, Particle, StringKind,
    Tables, TypeRow, ValueCheck, ValueClass, Versions, convert, part_roots, render, simple_types,
};
use crate::generate_schema::compare::transitional_only;
use crate::generate_schema::model::SchemaSet;
use crate::generate_schema::sdk_data::{DataChildKind, DataVersion, SdkData};

const STRICT_W: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

const NAMESPACES: &str = r#"[
  {"Prefix": "", "Uri": ""},
  {"Prefix": "m", "Uri": "http://schemas.openxmlformats.org/officeDocument/2006/math"},
  {"Prefix": "v", "Uri": "urn:schemas-microsoft-com:vml"},
  {"Prefix": "w", "Uri": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"},
  {"Prefix": "wps", "Uri": "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"}
]"#;

const TYPED_NAMESPACES: &str = r#"[
  {"Prefix": "m", "Namespace": "DocumentFormat.OpenXml.Math"},
  {"Prefix": "w", "Namespace": "DocumentFormat.OpenXml.Wordprocessing"}
]"#;

/// Excerpts of SDK 3.5.1's data at `v3.5.1`: the table-cell margins of
/// blueprint section 3.6 and their width types, `w:tblGridChange`, and two
/// enumerations, as the data writes them. Added for the tests: `w:wide`, a
/// derived type with an attribute of its own; `w:later`, `w:tblGridChange`
/// with its second bound at Office 2013, so that the replacement shows within
/// the six targets; `w:classes`, an attribute of each kind of class; and
/// `w:filtered` and `w:unfiltered`, `p:clrMru`'s groups with and without
/// `RequireFilter`.
const WORDPROCESSING: &str = r#"{"TargetNamespace": "http://schemas.openxmlformats.org/wordprocessingml/2006/main", "Types": [
  {"Name": "w:CT_TblCellMar/w:tblCellMar", "Children": [{"Name": "w:CT_TblWidth/w:top"}, {"Name": "w:CT_TblWidthDxaNil/w:left"}, {"Name": "w:CT_TblWidth/w:start"}, {"Name": "w:CT_TblWidth/w:bottom"}, {"Name": "w:CT_TblWidthDxaNil/w:right"}, {"Name": "w:CT_TblWidth/w:end"}],
   "Particle": {"Kind": "Sequence", "Items": [{"Name": "w:CT_TblWidth/w:top", "Occurs": [{"Max": 1}]}, {"Name": "w:CT_TblWidthDxaNil/w:left", "Occurs": [{"Max": 1}]}, {"Name": "w:CT_TblWidth/w:start", "Occurs": [{"Max": 1}], "InitialVersion": "Office2010"}, {"Name": "w:CT_TblWidth/w:bottom", "Occurs": [{"Max": 1}]}, {"Name": "w:CT_TblWidthDxaNil/w:right", "Occurs": [{"Max": 1}]}, {"Name": "w:CT_TblWidth/w:end", "Occurs": [{"Max": 1}], "InitialVersion": "Office2010"}]}},
  {"Name": "w:CT_TblWidth/", "IsAbstract": true, "Attributes": [{"QName": "w:w", "Type": "StringValue", "Validators": [
    {"Name": "NumberValidator", "Type": "w:ST_DecimalNumber", "Version": "Office2007"},
    {"Arguments": [{"Type": "String", "Name": "Pattern", "Value": "-?[0-9]+(\\.[0-9]+)?%"}], "Name": "StringValidator", "Version": "Office2010", "UnionId": 0},
    {"Name": "NumberValidator", "Type": "w:ST_DecimalNumber", "Version": "Office2010", "UnionId": 0},
    {"Arguments": [{"Type": "String", "Name": "Pattern", "Value": "-?[0-9]+(\\.[0-9]+)?%"}], "Name": "StringValidator", "Version": "Office2013", "UnionId": 1, "IsInitialVersion": true},
    {"Name": "NumberValidator", "Type": "w:ST_DecimalNumber", "Version": "Office2013", "UnionId": 1, "IsInitialVersion": true}]},
    {"QName": "w:type", "Type": "EnumValue<DocumentFormat.OpenXml.Wordprocessing.TableWidthUnitValues>"}]},
  {"Name": "w:CT_TblWidthDxaNil/", "IsAbstract": true, "Attributes": [
    {"QName": "w:w", "Type": "Int16Value", "Validators": [{"Name": "RequiredValidator"}, {"Arguments": [{"Type": "Long", "Name": "MinInclusive", "Value": "0"}], "Name": "NumberValidator", "IsInitialVersion": true}]},
    {"QName": "w:type", "Type": "EnumValue<DocumentFormat.OpenXml.Wordprocessing.TableWidthValues>", "Validators": [{"Name": "RequiredValidator"}]}]},
  {"Name": "w:CT_TblWidth/w:top", "IsDerived": true, "IsLeafElement": true},
  {"Name": "w:CT_TblWidth/w:start", "IsDerived": true, "IsLeafElement": true},
  {"Name": "w:CT_TblWidth/w:bottom", "IsDerived": true, "IsLeafElement": true},
  {"Name": "w:CT_TblWidth/w:end", "IsDerived": true, "IsLeafElement": true},
  {"Name": "w:CT_TblWidth/w:wide", "IsDerived": true, "IsLeafElement": true, "Attributes": [{"QName": "w:own", "Type": "BooleanValue"}]},
  {"Name": "w:CT_TblWidthDxaNil/w:left", "IsDerived": true, "IsLeafElement": true},
  {"Name": "w:CT_TblWidthDxaNil/w:right", "IsDerived": true, "IsLeafElement": true},
  {"Name": "w:CT_TblGridBase/w:tblGrid", "Children": [], "Particle": {"Kind": "Sequence", "Items": []}},
  {"Name": "w:CT_TblGridChange/w:tblGridChange", "Particle": {"Kind": "Sequence", "Items": [{"Kind": "Sequence", "Items": [{"Name": "w:CT_TblGridBase/w:tblGrid", "Occurs": [{"Min": 1, "Max": 1, "IncludeVersion": true}, {"Max": 1, "IncludeVersion": true, "Version": "Office2010"}]}]}]}},
  {"Name": "w:CT_TblGridChange/w:later", "Particle": {"Kind": "Sequence", "Items": [{"Kind": "Sequence", "Items": [{"Name": "w:CT_TblGridBase/w:tblGrid", "Occurs": [{"Min": 1, "Max": 1, "IncludeVersion": true}, {"Max": 1, "IncludeVersion": true, "Version": "Office2013"}]}]}]}},
  {"Name": "w:CT_Mru/w:filtered", "Particle": {"RequireFilter": true, "Kind": "Sequence", "Items": [{"Kind": "Group", "Occurs": [{"Max": 10, "IncludeVersion": true}, {"IncludeVersion": true, "Version": "Office2010"}], "Items": [{"Kind": "Choice", "Items": [{"Name": "w:CT_TblGridBase/w:tblGrid"}]}]}]}},
  {"Name": "w:CT_Mru/w:unfiltered", "Particle": {"Kind": "Sequence", "Items": [{"Kind": "Group", "Occurs": [{"Max": 10, "IncludeVersion": true}, {"IncludeVersion": true, "Version": "Office2010"}], "Items": [{"Kind": "Choice", "Items": [{"Name": "w:CT_TblGridBase/w:tblGrid"}]}]}]}},
  {"Name": "w:CT_Classes/w:classes", "Attributes": [
    {"QName": "w:string", "Type": "StringValue"},
    {"QName": "w:enum", "Type": "EnumValue<DocumentFormat.OpenXml.Wordprocessing.TableWidthValues>"},
    {"QName": "w:onOff", "Type": "OnOffValue"},
    {"QName": "w:blank", "Type": "TrueFalseBlankValue"},
    {"QName": "w:list", "Type": "ListValue<Int32Value>"},
    {"QName": ":boolean", "Type": "BooleanValue"},
    {"QName": "w:since", "Type": "HexBinaryValue", "Version": "Office2013"},
    {"QName": "w:token", "Type": "StringValue", "Validators": [{"Arguments": [{"Type": "Boolean", "Name": "IsToken", "Value": "True"}, {"Type": "Boolean", "Name": "IsNcName", "Value": "True"}], "Name": "StringValidator", "IsInitialVersion": true}]},
    {"QName": "w:id", "Type": "StringValue", "Validators": [{"Arguments": [{"Type": "Boolean", "Name": "IsToken", "Value": "True"}, {"Type": "Boolean", "Name": "IsNcName", "Value": "True"}, {"Type": "Boolean", "Name": "IsId", "Value": "True"}], "Name": "StringValidator", "IsInitialVersion": true}]},
    {"QName": "w:unit", "Type": "StringValue", "Validators": [{"Name": "EnumValidator", "Type": "w:ST_TblWidth", "IsInitialVersion": true}]}]},
  {"Name": "w:CT_Document/w:document"}
], "Enums": [
  {"Type": "w:ST_TblWidth", "Name": "TableWidthUnitValues", "Facets": [{"Value": "nil"}, {"Value": "pct"}, {"Value": "dxa"}, {"Value": "auto"}]},
  {"Type": "w:ST_TblWidthDxaNil", "Name": "TableWidthValues", "Facets": [{"Value": "nil"}, {"Value": "dxa", "Version": "Office2013"}]}
]}"#;

/// An excerpt of SDK 3.5.1's data at `v3.5.1`: `m:brkBinSub`, whose `m:val`
/// stops being required at Office 2010.
const MATH: &str = r#"{"TargetNamespace": "http://schemas.openxmlformats.org/officeDocument/2006/math", "Types": [
  {"Name": "m:CT_BreakBinSub/m:brkBinSub", "IsLeafElement": true, "Attributes": [{"QName": "m:val", "Type": "EnumValue<DocumentFormat.OpenXml.Math.BreakBinarySubtractionValues>", "Validators": [
    {"Name": "RequiredValidator", "Version": "Office2007"},
    {"Arguments": [{"Type": "Boolean", "Name": "IsRequired", "Value": "False"}], "Name": "RequiredValidator", "Version": "Office2010", "IsInitialVersion": true}]}]}
], "Enums": [
  {"Type": "m:ST_BreakBinSub", "Name": "BreakBinarySubtractionValues", "Facets": [{"Value": "--"}, {"Value": "-+"}, {"Value": "+-"}]}
]}"#;

/// An excerpt of SDK 3.5.1's data at `v3.5.1`: `w:shd` cut to `w:fill`, a
/// union of `auto` and a three-byte `w:ST_HexColorRGB`, and the main part's
/// root type.
const SHADING: &str = r#"{"TargetNamespace": "http://schemas.openxmlformats.org/wordprocessingml/2006/main", "Types": [
  {"Name": "w:CT_Shd/w:shd", "IsLeafElement": true, "Attributes": [{"QName": "w:fill", "Type": "StringValue", "Validators": [
    {"Name": "EnumValidator", "Type": "w:ST_HexColorAuto", "UnionId": 0, "IsInitialVersion": true},
    {"Arguments": [{"Type": "Long", "Name": "Length", "Value": "3"}], "Name": "StringValidator", "Type": "w:ST_HexColorRGB", "UnionId": 0, "IsInitialVersion": true}]}]},
  {"Name": "w:CT_Document/w:document"}
], "Enums": [
  {"Type": "w:ST_HexColorAuto", "Name": "AutomaticColorValues", "Facets": [{"Value": "auto"}]}
]}"#;

const MAIN_PART: &str = r#"{"Root": "w:CT_Document", "Name": "MainDocumentPart", "RelationshipType": "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument", "RootElement": "document", "Children": [
  {"MaxOccursGreatThanOne": true, "Name": "ImagePart"},
  {"MaxOccursGreatThanOne": true, "IsDataPartReference": true, "Name": "VideoReferenceRelationship"},
  {"MinOccursIsNonZero": true, "HasFixedContent": true, "Name": "ImagePart"}
]}"#;

const IMAGE_PART: &str = r#"{"Name": "ImagePart", "RelationshipType": "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"}"#;

/// The type of the root `StyleDefinitionsPart` takes from its base, and
/// `MainDocumentPart`'s.
const STYLES: &str = r#"{"TargetNamespace": "http://schemas.openxmlformats.org/wordprocessingml/2006/main", "Types": [
  {"Name": "w:CT_Styles/w:styles"},
  {"Name": "w:CT_Document/w:document"}
], "Enums": []}"#;

/// Three of the six parts with a `RootElement` and no `Root`, as the data
/// writes them, and a made-up one with no root type anywhere.
const ROOTLESS_PARTS: [(&str, &str); 4] = [
    (
        "StyleDefinitionsPart",
        r#"{"Name": "StyleDefinitionsPart", "Base": "StylesPart", "ContentType": "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml", "RelationshipType": "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles", "Target": "styles", "RootElement": "styles", "Paths": {"General": "."}}"#,
    ),
    (
        "CoreFilePropertiesPart",
        r#"{"Name": "CoreFilePropertiesPart", "Base": "OpenXmlPart", "ContentType": "application/vnd.openxmlformats-package.core-properties+xml", "RelationshipType": "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties", "Target": "core", "RootElement": "coreProperties", "Paths": {"General": "docProps"}}"#,
    ),
    (
        "XmlSignaturePart",
        r#"{"Name": "XmlSignaturePart", "Base": "OpenXmlPart", "ContentType": "application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml", "RelationshipType": "http://schemas.openxmlformats.org/package/2006/relationships/digital-signature/signature", "Target": "sig", "RootElement": "Signature", "Paths": {"General": "_xmlsignatures"}}"#,
    ),
    (
        "MadeUpPart",
        r#"{"Name": "MadeUpPart", "Base": "OpenXmlPart", "RootElement": "madeUp"}"#,
    ),
];

/// The SDK rows the marks are read for: margins with `w:left`, which Strict
/// lacks, `w:ind` with the attribute `w:left`, and `w:jc` of a table, whose
/// type and enumeration the SDK renames (`CT_TblJc`, `ST_TblJc`) from
/// ECMA-376's `CT_JcTable` and `ST_JcTable`.
const MARKED: &str = r#"{"TargetNamespace": "http://schemas.openxmlformats.org/wordprocessingml/2006/main", "Types": [
  {"Name": "w:CT_TblCellMar/w:tblCellMar", "Particle": {"Kind": "Sequence", "Items": [{"Name": "w:CT_TblWidth/w:top"}, {"Name": "w:CT_TblWidth/w:start"}, {"Name": "w:CT_TblWidth/w:left"}]}},
  {"Name": "w:CT_TblWidth/w:top"}, {"Name": "w:CT_TblWidth/w:start"}, {"Name": "w:CT_TblWidth/w:left"},
  {"Name": "w:CT_Ind/w:ind", "Attributes": [{"QName": "w:start", "Type": "Int32Value"}, {"QName": "w:left", "Type": "Int32Value"}]},
  {"Name": "w:CT_TblPr/w:tblPr", "Children": [{"Name": "w:CT_TblJc/w:jc"}]},
  {"Name": "w:CT_TblJc/w:jc", "Attributes": [{"QName": "w:val", "Type": "EnumValue<DocumentFormat.OpenXml.Wordprocessing.TableRowAlignmentValues>"}]},
  {"Name": "w:CT_Document/w:document"}
], "Enums": [
  {"Type": "w:ST_TblJc", "Name": "TableRowAlignmentValues", "Facets": [{"Value": "left"}, {"Value": "center"}]}
]}"#;

/// The SDK rows the Strict additions are read for: `w:b`, whose `w:val` is
/// `OnOffValue`, `w:ind`, and `w:txbxContent` in `wps:txbx`, a type of an
/// extension namespace that no schema file declares; and the main part's root
/// type.
const STRICT_MARKED: &str = r#"{"TargetNamespace": "http://schemas.openxmlformats.org/wordprocessingml/2006/main", "Types": [
  {"Name": "w:CT_OnOff/w:b", "IsLeafElement": true, "Attributes": [{"QName": "w:val", "Type": "OnOffValue"}]},
  {"Name": "w:CT_Ind/w:ind", "Attributes": [{"QName": "w:start", "Type": "Int32Value"}]},
  {"Name": "w:CT_TxbxContent/w:txbxContent"},
  {"Name": "wps:CT_TextboxInfo/wps:txbx", "Children": [{"Name": "w:CT_TxbxContent/w:txbxContent"}]},
  {"Name": "w:CT_Document/w:document"}
], "Enums": []}"#;

/// `vml-main.xsd`, which has no Strict twin, cut to one type.
const VML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="urn:schemas-microsoft-com:vml" targetNamespace="urn:schemas-microsoft-com:vml"
  elementFormDefault="qualified">
  <xsd:complexType name="CT_Shape"/>
</xsd:schema>"#;

/// `wml.xsd` of either class, cut to what the Strict additions need:
/// Transitional widens `ST_OnOff` with `on` and `off`, and declares
/// `CT_TxbxContent`; both type `CT_Ind`'s `start` alike.
fn strict_wml(namespace: &str, transitional: bool) -> String {
    let (members, extra) = if transitional {
        (
            "xsd:boolean ST_OnOff1",
            r#"<xsd:simpleType name="ST_OnOff1">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="on"/>
      <xsd:enumeration value="off"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:complexType name="CT_TxbxContent"/>"#,
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
  {extra}
  <xsd:complexType name="CT_OnOff">
    <xsd:attribute name="val" type="ST_OnOff"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Ind">
    <xsd:attribute name="start" type="xsd:integer"/>
  </xsd:complexType>
</xsd:schema>"#
    )
}

/// `wml.xsd` of either class, cut to what the marks need: Transitional adds
/// the element `left`, the enumeration value `left`, and the attribute `left`.
fn wml(namespace: &str, transitional: bool) -> String {
    let (element, value, attribute) = if transitional {
        (
            r#"<xsd:element name="left" type="CT_TblWidth" minOccurs="0"/>"#,
            r#"<xsd:enumeration value="left"/>"#,
            r#"<xsd:attribute name="left" type="xsd:integer"/>"#,
        )
    } else {
        ("", "", "")
    };
    format!(
        r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns="{namespace}"
  targetNamespace="{namespace}" elementFormDefault="qualified" attributeFormDefault="qualified">
  <xsd:complexType name="CT_TblWidth"/>
  <xsd:complexType name="CT_TblCellMar">
    <xsd:sequence>
      <xsd:element name="top" type="CT_TblWidth" minOccurs="0"/>
      <xsd:element name="start" type="CT_TblWidth" minOccurs="0"/>
      {element}
    </xsd:sequence>
  </xsd:complexType>
  <xsd:simpleType name="ST_JcTable">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="center"/>
      {value}
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:complexType name="CT_JcTable">
    <xsd:attribute name="val" type="ST_JcTable" use="required"/>
  </xsd:complexType>
  <xsd:complexType name="CT_TblPr">
    <xsd:sequence>
      <xsd:element name="jc" type="CT_JcTable" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Ind">
    <xsd:attribute name="start" type="xsd:integer"/>
    {attribute}
  </xsd:complexType>
</xsd:schema>"#
    )
}

fn attribute<'tables>(row: &'tables TypeRow, name: &str) -> &'tables AttributeRow {
    row.attributes
        .iter()
        .find(|attribute| attribute.name == name)
        .unwrap()
}

fn data(schemas: &[&str]) -> SdkData {
    data_with_parts(schemas, &[])
}

fn data_with_parts(schemas: &[&str], parts: &[(&str, &str)]) -> SdkData {
    let base = MemorySystem::new()
        .with_file("data/namespaces.json", NAMESPACES.as_bytes())
        .unwrap()
        .with_file("data/typed/namespaces.json", TYPED_NAMESPACES.as_bytes())
        .unwrap()
        .with_file("data/parts/ImagePart.json", IMAGE_PART.as_bytes())
        .unwrap()
        .with_file("data/parts/MainDocumentPart.json", MAIN_PART.as_bytes())
        .unwrap()
        .with_file("data/schematrons.json", b"[]")
        .unwrap();
    let with_schemas = schemas
        .iter()
        .enumerate()
        .fold(base, |files, (index, schema)| {
            files
                .with_file(format!("data/schemas/{index}.json"), schema.as_bytes())
                .unwrap()
        });
    let system = parts.iter().fold(with_schemas, |files, (name, part)| {
        files
            .with_file(format!("data/parts/{name}.json"), part.as_bytes())
            .unwrap()
    });
    SdkData::read(&system, Path::new("data")).unwrap()
}

fn element(type_name: &str, min: u32, max: u32) -> Particle {
    Particle::Element {
        max,
        min,
        type_name: type_name.to_owned(),
    }
}

fn every_target(check: Check) -> ValueCheck {
    ValueCheck {
        check,
        union: None,
        versions: Versions::ALL,
    }
}

fn number(class: ValueClass, min_inclusive: Option<i64>) -> Check {
    Check::Number {
        bounds: Bounds {
            min_inclusive,
            ..Bounds::default()
        },
        class,
        list: false,
    }
}

fn part_row<'tables>(tables: &'tables Tables, name: &str) -> &'tables PartRow {
    tables.parts.iter().find(|part| part.name == name).unwrap()
}

/// The tables of `MATH` and `SHADING`, `w:ST_HexColorRGB` read as a `HexBinaryValue`.
fn regressions() -> Tables {
    let hex = simple_types(r#""w:ST_HexColorRGB" = "HexBinaryValue""#).unwrap();
    convert(
        &data(&[MATH, SHADING]),
        &hex,
        &BTreeMap::new(),
        &Marks::default(),
    )
    .unwrap()
}

/// The tables of `STYLES` with `parts`, `StylesPart` in the table of part roots.
fn rooted(parts: &[(&str, &str)]) -> anyhow::Result<Tables> {
    let roots = part_roots(r#""StylesPart" = "w:CT_Styles/w:styles""#)?;
    convert(
        &data_with_parts(&[STYLES], parts),
        &BTreeMap::new(),
        &roots,
        &Marks::default(),
    )
}

fn row<'tables>(tables: &'tables Tables, name: &str) -> &'tables TypeRow {
    tables.types.iter().find(|row| row.name == name).unwrap()
}

fn string(pattern: Option<&str>) -> Check {
    Check::String {
        class: None,
        kind: StringKind::Plain,
        length: None,
        max_length: None,
        min_length: None,
        pattern: pattern.map(str::to_owned),
    }
}

fn tables() -> Tables {
    let decimal = simple_types(r#""w:ST_DecimalNumber" = "Int32Value""#).unwrap();
    convert(
        &data(&[WORDPROCESSING]),
        &decimal,
        &BTreeMap::new(),
        &Marks::default(),
    )
    .unwrap()
}

#[test]
fn a_derived_type_takes_its_schema_types_attributes_first_and_its_checks_in_order() {
    let tables = tables();
    let names: Vec<&str> = row(&tables, "w:CT_TblWidth/w:wide")
        .attributes
        .iter()
        .map(|attribute| attribute.name.as_str())
        .collect();
    assert_eq!(names, ["w", "type", "own"]);
    let left = row(&tables, "w:CT_TblWidthDxaNil/w:left");
    let width = attribute(left, "w");
    assert_eq!(
        (width.class.clone(), width.prefix.as_str()),
        (ValueClass::Int16, "w")
    );
    assert_eq!(
        width.checks,
        [
            every_target(Check::Required),
            every_target(number(ValueClass::Int16, Some(0))),
            every_target(number(ValueClass::Int16, None)),
        ]
    );
}

#[test]
fn a_validator_of_office_2007_alone_runs_at_no_target_and_an_initial_version_from_it_on() {
    let tables = tables();
    let versions: Vec<(Option<u8>, Versions)> = attribute(row(&tables, "w:CT_TblWidth/w:top"), "w")
        .checks
        .iter()
        .map(|check| (check.union, check.versions))
        .collect();
    assert_eq!(
        versions,
        [
            (None, Versions::NONE),
            (Some(0), Versions(0b00_0001)),
            (Some(0), Versions(0b00_0001)),
            (Some(1), Versions(0b11_1110)),
            (Some(1), Versions(0b11_1110)),
            (None, Versions::ALL),
        ]
    );
    assert_eq!(Versions::since(DataVersion::Office2007), Versions::ALL);
}

#[test]
fn the_table_cell_margins_convert_as_blueprint_section_3_6_gives_them() {
    let tables = tables();
    for side in ["w:CT_TblWidthDxaNil/w:left", "w:CT_TblWidthDxaNil/w:right"] {
        let width = attribute(row(&tables, side), "w");
        assert_eq!(width.class, ValueClass::Int16, "{side}");
        assert_eq!(
            width.checks,
            [
                every_target(Check::Required),
                every_target(number(ValueClass::Int16, Some(0))),
                every_target(number(ValueClass::Int16, None)),
            ],
            "{side}"
        );
    }
    let percent = string(Some("-?[0-9]+(\\.[0-9]+)?%"));
    let int32 = number(ValueClass::Int32, None);
    let check = |check: &Check, union: Option<u8>, versions: u8| ValueCheck {
        check: check.clone(),
        union,
        versions: Versions(versions),
    };
    for side in ["w:CT_TblWidth/w:top", "w:CT_TblWidth/w:bottom"] {
        let width = attribute(row(&tables, side), "w");
        assert_eq!(width.class, ValueClass::String, "{side}");
        assert_eq!(
            width.checks,
            [
                check(&int32, None, 0b00_0000),
                check(&percent, Some(0), 0b00_0001),
                check(&int32, Some(0), 0b00_0001),
                check(&percent, Some(1), 0b11_1110),
                check(&int32, Some(1), 0b11_1110),
                every_target(string(None)),
            ],
            "{side}"
        );
    }
    let margins = row(&tables, "w:CT_TblCellMar/w:tblCellMar");
    assert_eq!(
        margins.particles,
        [(
            Versions::ALL,
            Some(Particle::Composite {
                items: vec![
                    element("w:CT_TblWidth/w:top", 0, 1),
                    element("w:CT_TblWidthDxaNil/w:left", 0, 1),
                    element("w:CT_TblWidth/w:start", 0, 1),
                    element("w:CT_TblWidth/w:bottom", 0, 1),
                    element("w:CT_TblWidthDxaNil/w:right", 0, 1),
                    element("w:CT_TblWidth/w:end", 0, 1),
                ],
                kind: CompositeKind::Sequence,
                max: 1,
                min: 1,
            })
        )]
    );
    let children: Vec<(&str, &str)> = margins
        .children
        .iter()
        .map(|child| (child.local.as_str(), child.type_name.as_str()))
        .collect();
    assert_eq!(
        children,
        [
            ("bottom", "w:CT_TblWidth/w:bottom"),
            ("end", "w:CT_TblWidth/w:end"),
            ("left", "w:CT_TblWidthDxaNil/w:left"),
            ("right", "w:CT_TblWidthDxaNil/w:right"),
            ("start", "w:CT_TblWidth/w:start"),
            ("top", "w:CT_TblWidth/w:top"),
        ]
    );
}

#[test]
fn a_required_attribute_a_bound_and_an_enumeration_member_are_kept() {
    let tables = tables();
    let left = row(&tables, "w:CT_TblWidthDxaNil/w:left");
    let unit = attribute(left, "type");
    assert_eq!(
        unit.class,
        ValueClass::Enum("w:ST_TblWidthDxaNil".to_owned())
    );
    assert_eq!(
        unit.checks,
        [
            every_target(Check::Required),
            every_target(Check::Enum {
                class: ValueClass::Enum("w:ST_TblWidthDxaNil".to_owned()),
            }),
        ]
    );
    let bounds = attribute(left, "w").checks.iter().find_map(|check| {
        if let Check::Number {
            bounds: limits,
            class: _class,
            list: _list,
        } = &check.check
            && *limits != Bounds::default()
        {
            Some(*limits)
        } else {
            None
        }
    });
    assert_eq!(bounds.and_then(|found| found.min_inclusive), Some(0));
    let members = &tables
        .enumerations
        .iter()
        .find(|enumeration| enumeration.simple_type == "w:ST_TblWidthDxaNil")
        .unwrap()
        .members;
    assert_eq!(
        members,
        &[
            ("nil".to_owned(), Versions::ALL, false),
            ("dxa".to_owned(), Versions(0b11_1110), false),
        ]
    );
}

#[test]
fn a_required_validator_with_is_required_false_emits_no_check() {
    let tables = regressions();
    let value = attribute(row(&tables, "m:CT_BreakBinSub/m:brkBinSub"), "val");
    let class = ValueClass::Enum("m:ST_BreakBinSub".to_owned());
    assert_eq!(
        value.checks,
        [
            ValueCheck {
                check: Check::Required,
                union: None,
                versions: Versions::NONE,
            },
            every_target(Check::Enum { class }),
        ],
        "the Office 2007 requirement alone, at no target"
    );
}

#[test]
fn a_hex_binary_string_validator_keeps_its_class() {
    let tables = regressions();
    let union = |check: Check| ValueCheck {
        check,
        union: Some(0),
        versions: Versions::ALL,
    };
    assert_eq!(
        attribute(row(&tables, "w:CT_Shd/w:shd"), "fill").checks,
        [
            union(Check::Enum {
                class: ValueClass::Enum("w:ST_HexColorAuto".to_owned()),
            }),
            union(Check::String {
                class: Some(ValueClass::HexBinary),
                kind: StringKind::Plain,
                length: Some(3),
                max_length: None,
                min_length: None,
                pattern: None,
            }),
            every_target(string(None)),
        ]
    );
}

#[test]
fn a_string_validator_of_another_class_stops_the_generator() {
    let decimal = simple_types(r#""w:ST_DecimalNumber" = "Int32Value""#).unwrap();
    let schema = SHADING.replace("w:ST_HexColorRGB", "w:ST_DecimalNumber");
    let error = convert(
        &data(&[&schema]),
        &decimal,
        &BTreeMap::new(),
        &Marks::default(),
    )
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("w:ST_DecimalNumber reads as Int32"),
        "{error:#}"
    );
}

#[test]
fn a_content_model_is_built_per_target_with_a_later_bound_replacing_an_earlier_one() {
    let tables = tables();
    let grid = |min: u32| Particle::Composite {
        items: vec![Particle::Composite {
            items: vec![element("w:CT_TblGridBase/w:tblGrid", min, 1)],
            kind: CompositeKind::Sequence,
            max: 1,
            min: 1,
        }],
        kind: CompositeKind::Sequence,
        max: 1,
        min: 1,
    };
    assert_eq!(
        row(&tables, "w:CT_TblGridChange/w:tblGridChange").particles,
        [(Versions::ALL, Some(grid(0)))],
        "the Office 2010 bound at every target"
    );
    assert_eq!(
        row(&tables, "w:CT_TblGridChange/w:later").particles,
        [
            (Versions(0b00_0001), Some(grid(1))),
            (Versions(0b11_1110), Some(grid(0))),
        ],
        "required at Office 2010, optional from Office 2013 on"
    );
}

#[test]
fn a_later_group_replaces_an_earlier_one_only_under_require_filter() {
    let tables = tables();
    let group = |max: u32| Particle::Composite {
        items: vec![Particle::Composite {
            items: vec![element("w:CT_TblGridBase/w:tblGrid", 1, 1)],
            kind: CompositeKind::Choice,
            max: 1,
            min: 1,
        }],
        kind: CompositeKind::Group,
        max,
        min: 0,
    };
    let sequence = |items: Vec<Particle>| Particle::Composite {
        items,
        kind: CompositeKind::Sequence,
        max: 1,
        min: 1,
    };
    assert_eq!(
        row(&tables, "w:CT_Mru/w:filtered").particles,
        [(Versions::ALL, Some(sequence(vec![group(0)])))]
    );
    assert_eq!(
        row(&tables, "w:CT_Mru/w:unfiltered").particles,
        [(Versions::ALL, Some(sequence(vec![group(10), group(0)])))]
    );
}

#[test]
fn each_class_gets_the_check_sdk_3_5_1_adds_and_a_version_its_office_version_check() {
    let tables = tables();
    let classes = row(&tables, "w:CT_Classes/w:classes");
    let checks: Vec<(&str, &str, Vec<Check>)> = classes
        .attributes
        .iter()
        .map(|attribute| {
            (
                attribute.prefix.as_str(),
                attribute.name.as_str(),
                attribute
                    .checks
                    .iter()
                    .map(|check| check.check.clone())
                    .collect(),
            )
        })
        .collect();
    let token = |kind: StringKind| Check::String {
        class: None,
        kind,
        length: None,
        max_length: None,
        min_length: None,
        pattern: None,
    };
    let enumeration = ValueClass::Enum("w:ST_TblWidthDxaNil".to_owned());
    let unit = ValueClass::Enum("w:ST_TblWidth".to_owned());
    assert_eq!(
        checks,
        [
            ("w", "string", vec![string(None)]),
            ("w", "enum", vec![Check::Enum { class: enumeration }]),
            (
                "w",
                "onOff",
                vec![Check::Enum {
                    class: ValueClass::OnOff
                }]
            ),
            (
                "w",
                "blank",
                vec![Check::Enum {
                    class: ValueClass::TrueFalseBlank
                }]
            ),
            (
                "w",
                "list",
                vec![Check::List {
                    item: Box::new(number(ValueClass::Int32, None)),
                }]
            ),
            ("", "boolean", vec![number(ValueClass::Boolean, None)]),
            (
                "w",
                "since",
                vec![Check::OfficeVersion, number(ValueClass::HexBinary, None)]
            ),
            ("w", "token", vec![token(StringKind::NcName), string(None)]),
            ("w", "id", vec![token(StringKind::Id), string(None)]),
            ("w", "unit", vec![Check::Enum { class: unit }, string(None)]),
        ]
    );
    let since = attribute(classes, "since");
    assert_eq!(since.checks[0].versions, Versions(0b11_1110));
}

#[test]
fn part_rules_keep_their_children_and_their_root_type() {
    let tables = tables();
    let main = tables
        .parts
        .iter()
        .find(|part| part.name == "MainDocumentPart")
        .unwrap();
    assert_eq!(main.root.as_deref(), Some("w:CT_Document/w:document"));
    let children: Vec<(&str, DataChildKind, bool, bool)> = main
        .children
        .iter()
        .map(|child| (child.name.as_str(), child.kind, child.many, child.required))
        .collect();
    assert_eq!(
        children,
        [
            ("ImagePart", DataChildKind::Ordinary, true, false),
            (
                "VideoReferenceRelationship",
                DataChildKind::DataPart,
                true,
                false
            ),
            ("ImagePart", DataChildKind::Ordinary, false, true),
        ]
    );
    let summary = tables.summary(&Marks::default());
    assert_eq!(
        (
            summary.parts,
            summary.part_children,
            summary.part_required,
            summary.part_once
        ),
        (2, 3, 1, 1)
    );
}

#[test]
fn a_part_takes_the_root_of_its_abstract_base() {
    let tables = rooted(&ROOTLESS_PARTS[..3]).unwrap();
    assert_eq!(
        part_row(&tables, "StyleDefinitionsPart").root.as_deref(),
        Some("w:CT_Styles/w:styles")
    );
}

#[test]
fn a_part_with_a_root_element_and_no_root_type_stops_the_generator() {
    let error = rooted(&ROOTLESS_PARTS).unwrap_err();
    let message = format!("{error:#}");
    assert!(
        message.contains("the part MadeUpPart has the root element madeUp and no root type"),
        "{message}"
    );
}

#[test]
fn core_properties_and_xml_signature_keep_no_root() {
    let tables = rooted(&ROOTLESS_PARTS[..3]).unwrap();
    for name in ["CoreFilePropertiesPart", "XmlSignaturePart"] {
        assert_eq!(part_row(&tables, name).root, None, "{name}");
    }
}

#[test]
fn every_transitional_only_item_of_the_comparison_is_marked() {
    let system = MemorySystem::new()
        .with_file("t/wml.xsd", wml(W, true).as_bytes())
        .unwrap()
        .with_file("s/wml.xsd", wml(STRICT_W, false).as_bytes())
        .unwrap();
    let transitional = SchemaSet::read(&system, &["t/wml.xsd"]).unwrap();
    let strict = SchemaSet::read(&system, &["s/wml.xsd"]).unwrap();
    let marks = Marks::new(&transitional, &transitional_only(&transitional, &strict));
    let tables = convert(&data(&[MARKED]), &BTreeMap::new(), &BTreeMap::new(), &marks).unwrap();

    let children: Vec<(&str, bool)> = row(&tables, "w:CT_TblCellMar/w:tblCellMar")
        .children
        .iter()
        .map(|child: &ChildRow| (child.local.as_str(), child.transitional_only))
        .collect();
    assert_eq!(children, [("left", true), ("start", false), ("top", false)]);
    let attributes: Vec<(&str, bool)> = row(&tables, "w:CT_Ind/w:ind")
        .attributes
        .iter()
        .map(|attribute| (attribute.name.as_str(), attribute.transitional_only))
        .collect();
    assert_eq!(attributes, [("start", false), ("left", true)]);
    let members: Vec<(&str, bool)> = tables
        .enumerations
        .iter()
        .flat_map(|enumeration| &enumeration.members)
        .map(|(value, _, transitional_only)| (value.as_str(), *transitional_only))
        .collect();
    assert_eq!(
        members,
        [("left", true), ("center", false)],
        "through the renamed CT_TblJc"
    );
    assert_eq!(tables.summary(&marks).unmatched, Vec::<String>::new());
}

#[test]
fn the_strict_set_adds_strict_checks_namespaces_and_transitional_only_types() {
    let system = MemorySystem::new()
        .with_file("t/wml.xsd", strict_wml(W, true).as_bytes())
        .unwrap()
        .with_file("t/vml-main.xsd", VML.as_bytes())
        .unwrap()
        .with_file("s/wml.xsd", strict_wml(STRICT_W, false).as_bytes())
        .unwrap();
    let transitional = SchemaSet::read(&system, &["t/vml-main.xsd", "t/wml.xsd"]).unwrap();
    let strict = SchemaSet::read(&system, &["s/wml.xsd"]).unwrap();
    let marks = Marks::new(&transitional, &transitional_only(&transitional, &strict))
        .with_strict(&transitional, &strict)
        .unwrap();
    let tables = convert(
        &data(&[STRICT_MARKED]),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &marks,
    )
    .unwrap();

    let on_off = attribute(row(&tables, "w:CT_OnOff/w:b"), "val");
    assert_eq!(
        on_off.strict_checks,
        Some(vec![every_target(Check::Enum {
            class: ValueClass::Boolean,
        })])
    );
    assert_eq!(
        attribute(row(&tables, "w:CT_Ind/w:ind"), "start").strict_checks,
        None,
        "both classes type it alike"
    );
    let children: Vec<(&str, bool)> = row(&tables, "wps:CT_TextboxInfo/wps:txbx")
        .children
        .iter()
        .map(|child| (child.local.as_str(), child.transitional_only))
        .collect();
    assert_eq!(
        children,
        [("txbxContent", true)],
        "a Transitional-only type, under a parent no schema file declares"
    );
    assert_eq!(
        tables.transitional_only_namespaces,
        [("v".to_owned(), "urn:schemas-microsoft-com:vml".to_owned())]
    );
    let summary = tables.summary(&marks);
    assert_eq!((summary.strict_checks, summary.marked_children), (1, 1));

    let files = render::files("// header\n", &tables, &["", "m", "v", "w", "wps"]).unwrap();
    assert!(
        files[1]
            .1
            .contains("const STRICT_CHECKS_1: &[ValueCheck] = &[ValueCheck {\n")
    );
    assert!(files[1].1.contains("strict_checks: Some(STRICT_CHECKS_1),"));
    assert!(
        files[5]
            .1
            .contains("pub static TRANSITIONAL_ONLY_NAMESPACES: &[NamespaceId] = &[2];")
    );
}

#[test]
fn the_table_of_simple_types_is_toml_of_class_names() {
    let table = simple_types("# a comment\n\"w:ST_DecimalNumber\" = \"Int32Value\"\n").unwrap();
    assert_eq!(
        table.into_iter().collect::<Vec<_>>(),
        [("w:ST_DecimalNumber".to_owned(), "Int32Value".to_owned())]
    );
    let error = simple_types("\"w:ST_DecimalNumber\" = 3").unwrap_err();
    assert!(error.to_string().contains("w:ST_DecimalNumber"), "{error}");
}

#[test]
fn the_files_start_with_the_header_and_write_numbers_as_clippy_reads_them() {
    let mut tables = tables();
    tables
        .types
        .retain(|row| row.name == "w:CT_TblWidthDxaNil/w:left");
    let left = &mut tables.types[0];
    left.attributes.truncate(1);
    left.attributes[0].checks = vec![every_target(Check::Number {
        bounds: Bounds {
            max_inclusive: Some(27_273_042_316_900),
            min_inclusive: Some(-31_680),
            ..Bounds::default()
        },
        class: ValueClass::Int64,
        list: false,
    })];
    left.particles = vec![(
        Versions::ALL,
        Some(Particle::Composite {
            items: vec![],
            kind: CompositeKind::Sequence,
            max: 0xFFFF,
            min: 65_430,
        }),
    )];
    tables.parts.clear();
    let files = render::files("// header\n", &tables, &["w"]).unwrap();
    let names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        [
            "generated.rs",
            "attributes.rs",
            "enums.rs",
            "particles.rs",
            "parts.rs",
            "types.rs"
        ]
    );
    assert!(
        files
            .iter()
            .all(|(_, text)| text.starts_with("// header\n\n"))
    );
    let attributes = &files[1].1;
    assert!(attributes.contains("const CHECKS_1: &[ValueCheck] = &[ValueCheck {\n"));
    assert!(attributes.contains("max_inclusive: Some(27_273_042_316_900.0_f64),"));
    assert!(attributes.contains("min_inclusive: Some(-31680.0_f64),"));
    assert!(attributes.contains("namespace: Some(0),"));
    let particles = &files[3].1;
    assert!(particles.contains("max: 0xFFFF,"));
    assert!(particles.contains("min: 65430,"));
    assert!(
        files[5]
            .1
            .contains("pub fn type_id(name: &str) -> Option<TypeId> {")
    );
}
