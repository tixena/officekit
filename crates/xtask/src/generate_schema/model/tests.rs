use std::path::Path;

use os_shim::mock::MemorySystem;

use super::{
    AttributeUse, ComplexType, Content, Declarations, Derivation, Element, Facets, Method, Name,
    Particle, ParticleKind, SchemaSet, SimpleType, TypeRef,
};

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const S: &str = "http://schemas.openxmlformats.org/officeDocument/2006/sharedTypes";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const XML: &str = "http://www.w3.org/XML/1998/namespace";

/// Excerpts of `wml.xsd`, the types they name reduced to stand-ins.
const WML: &str = r##"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:s="http://schemas.openxmlformats.org/officeDocument/2006/sharedTypes"
  xmlns:v="urn:schemas-microsoft-com:vml"
  xmlns="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  elementFormDefault="qualified" attributeFormDefault="qualified" blockDefault="#all"
  targetNamespace="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <xsd:import namespace="http://schemas.openxmlformats.org/officeDocument/2006/math"
    schemaLocation="shared-math.xsd"/>
  <xsd:import namespace="http://schemas.openxmlformats.org/officeDocument/2006/sharedTypes"
    schemaLocation="shared-commonSimpleTypes.xsd"/>
  <xsd:import namespace="http://www.w3.org/XML/1998/namespace"/>
  <xsd:simpleType name="ST_LongHexNumber">
    <xsd:restriction base="xsd:hexBinary">
      <xsd:length value="4"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_DecimalNumberOrPercent">
    <xsd:union memberTypes="ST_UnqualifiedPercentage s:ST_Percentage"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_UnqualifiedPercentage">
    <xsd:restriction base="xsd:integer"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_DecimalNumber">
    <xsd:restriction base="xsd:integer"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_DocProtect">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="none"/>
      <xsd:enumeration value="readOnly"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:attributeGroup name="AG_Password">
    <xsd:attribute name="algorithmName" type="s:ST_String" use="optional"/>
    <xsd:attribute name="hashValue" type="xsd:base64Binary" use="optional"/>
    <xsd:attribute name="saltValue" type="xsd:base64Binary" use="optional"/>
    <xsd:attribute name="spinCount" type="ST_DecimalNumber" use="optional"/>
  </xsd:attributeGroup>
  <xsd:attributeGroup name="AG_TransitionalPassword">
    <xsd:attribute name="cryptProviderType" type="s:ST_CryptProv"/>
    <xsd:attribute name="cryptAlgorithmClass" type="s:ST_AlgClass"/>
    <xsd:attribute name="cryptAlgorithmType" type="s:ST_AlgType"/>
    <xsd:attribute name="cryptAlgorithmSid" type="ST_DecimalNumber"/>
    <xsd:attribute name="cryptSpinCount" type="ST_DecimalNumber"/>
    <xsd:attribute name="cryptProvider" type="s:ST_String"/>
    <xsd:attribute name="algIdExt" type="ST_LongHexNumber"/>
    <xsd:attribute name="algIdExtSource" type="s:ST_String"/>
    <xsd:attribute name="cryptProviderTypeExt" type="ST_LongHexNumber"/>
    <xsd:attribute name="cryptProviderTypeExtSource" type="s:ST_String"/>
    <xsd:attribute name="hash" type="xsd:base64Binary"/>
    <xsd:attribute name="salt" type="xsd:base64Binary"/>
  </xsd:attributeGroup>
  <xsd:complexType name="CT_DocProtect">
    <xsd:attribute name="edit" type="ST_DocProtect" use="optional"/>
    <xsd:attribute name="formatting" type="s:ST_OnOff" use="optional"/>
    <xsd:attribute name="enforcement" type="s:ST_OnOff"/>
    <xsd:attributeGroup ref="AG_Password"/>
    <xsd:attributeGroup ref="AG_TransitionalPassword"/>
  </xsd:complexType>
  <xsd:complexType name="CT_Markup">
    <xsd:attribute name="id" type="ST_DecimalNumber" use="required"/>
  </xsd:complexType>
  <xsd:complexType name="CT_TrackChange">
    <xsd:complexContent>
      <xsd:extension base="CT_Markup">
        <xsd:attribute name="author" type="s:ST_String" use="required"/>
        <xsd:attribute name="date" type="xsd:dateTime" use="optional"/>
      </xsd:extension>
    </xsd:complexContent>
  </xsd:complexType>
  <xsd:complexType name="CT_Comment">
    <xsd:complexContent>
      <xsd:extension base="CT_TrackChange">
        <xsd:sequence>
          <xsd:group ref="EG_BlockLevelElts" minOccurs="0" maxOccurs="unbounded"/>
        </xsd:sequence>
        <xsd:attribute name="initials" type="s:ST_String" use="optional"/>
      </xsd:extension>
    </xsd:complexContent>
  </xsd:complexType>
  <xsd:complexType name="CT_PPrBase">
    <xsd:sequence>
      <xsd:element name="keepNext" type="CT_OnOff" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_PPr">
    <xsd:complexContent>
      <xsd:extension base="CT_PPrBase">
        <xsd:sequence>
          <xsd:element name="rPr" type="CT_OnOff" minOccurs="0"/>
        </xsd:sequence>
      </xsd:extension>
    </xsd:complexContent>
  </xsd:complexType>
  <xsd:complexType name="CT_OnOff">
    <xsd:attribute name="val" type="s:ST_OnOff" default="true"/>
  </xsd:complexType>
  <xsd:complexType name="CT_StrictOnOff">
    <xsd:complexContent>
      <xsd:restriction base="CT_OnOff">
        <xsd:sequence>
          <xsd:element name="note" type="xsd:string"/>
        </xsd:sequence>
        <xsd:attribute name="val" type="s:ST_OnOff" use="required"/>
      </xsd:restriction>
    </xsd:complexContent>
  </xsd:complexType>
  <xsd:group name="EG_BlockLevelChunkElts">
    <xsd:choice>
      <xsd:element name="p" type="CT_P"/>
    </xsd:choice>
  </xsd:group>
  <xsd:group name="EG_BlockLevelElts">
    <xsd:choice>
      <xsd:group ref="EG_BlockLevelChunkElts" minOccurs="0" maxOccurs="unbounded"/>
      <xsd:element name="altChunk" type="CT_AltChunk" minOccurs="0" maxOccurs="unbounded"/>
    </xsd:choice>
  </xsd:group>
  <xsd:group name="EG_PContent">
    <xsd:choice>
      <xsd:element ref="m:oMath"/>
    </xsd:choice>
  </xsd:group>
  <xsd:complexType name="CT_AltChunk">
    <xsd:attribute ref="r:id" use="optional"/>
  </xsd:complexType>
  <xsd:complexType name="CT_P">
    <xsd:sequence>
      <xsd:element name="pPr" type="CT_PPr" minOccurs="0"/>
      <xsd:group ref="EG_PContent" minOccurs="0" maxOccurs="unbounded"/>
    </xsd:sequence>
    <xsd:attribute name="rsidRPr" type="ST_LongHexNumber"/>
    <xsd:attribute name="rsidR" type="ST_LongHexNumber"/>
    <xsd:attribute name="rsidDel" type="ST_LongHexNumber"/>
    <xsd:attribute name="rsidP" type="ST_LongHexNumber"/>
    <xsd:attribute name="rsidRDefault" type="ST_LongHexNumber"/>
  </xsd:complexType>
  <xsd:simpleType name="ST_MeasurementOrPercent">
    <xsd:union memberTypes="ST_DecimalNumberOrPercent s:ST_UniversalMeasure"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_TblWidth">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="pct"/>
      <xsd:enumeration value="dxa"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:complexType name="CT_TblWidth">
    <xsd:attribute name="w" type="ST_MeasurementOrPercent"/>
    <xsd:attribute name="type" type="ST_TblWidth"/>
  </xsd:complexType>
  <xsd:complexType name="CT_TblCellMar">
    <xsd:sequence>
      <xsd:element name="top" type="CT_TblWidth" minOccurs="0" maxOccurs="1"/>
      <xsd:element name="start" type="CT_TblWidth" minOccurs="0" maxOccurs="1"/>
      <xsd:element name="left" type="CT_TblWidth" minOccurs="0"/>
      <xsd:element name="bottom" type="CT_TblWidth" minOccurs="0" maxOccurs="1"/>
      <xsd:element name="end" type="CT_TblWidth" minOccurs="0" maxOccurs="1"/>
      <xsd:element name="right" type="CT_TblWidth" minOccurs="0"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Text">
    <xsd:simpleContent>
      <xsd:extension base="s:ST_String">
        <xsd:attribute ref="xml:space" use="optional"/>
      </xsd:extension>
    </xsd:simpleContent>
  </xsd:complexType>
  <xsd:complexType name="CT_ShortText">
    <xsd:simpleContent>
      <xsd:restriction base="CT_Text">
        <xsd:maxLength value="8"/>
      </xsd:restriction>
    </xsd:simpleContent>
  </xsd:complexType>
  <xsd:complexType name="CT_Picture" mixed="true">
    <xsd:sequence>
      <xsd:element ref="v:shape"/>
      <xsd:element name="extLst" type="v:CT_Extension"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:element name="document">
    <xsd:complexType>
      <xsd:sequence>
        <xsd:element name="p" type="CT_P"/>
      </xsd:sequence>
    </xsd:complexType>
  </xsd:element>
</xsd:schema>"##;

/// Excerpts of `shared-commonSimpleTypes.xsd`.
const SHARED: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://schemas.openxmlformats.org/officeDocument/2006/sharedTypes"
  targetNamespace="http://schemas.openxmlformats.org/officeDocument/2006/sharedTypes"
  elementFormDefault="qualified">
  <xsd:simpleType name="ST_String">
    <xsd:restriction base="xsd:string"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_OnOff">
    <xsd:union memberTypes="xsd:boolean ST_OnOff1"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_OnOff1">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="on"/>
      <xsd:enumeration value="off"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_UniversalMeasure">
    <xsd:restriction base="xsd:string">
      <xsd:pattern value="-?[0-9]+(\.[0-9]+)?(mm|cm|in|pt|pc|pi)"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_Percentage">
    <xsd:restriction base="xsd:string">
      <xsd:pattern value="-?[0-9]+(\.[0-9]+)?%"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_CryptProv">
    <xsd:restriction base="xsd:string"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_AlgClass">
    <xsd:restriction base="xsd:string"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_AlgType">
    <xsd:restriction base="xsd:string"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_RelationshipId">
    <xsd:restriction base="xsd:string"/>
  </xsd:simpleType>
</xsd:schema>"#;

/// Excerpts of `shared-math.xsd`.
const MATH: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://schemas.openxmlformats.org/officeDocument/2006/math"
  targetNamespace="http://schemas.openxmlformats.org/officeDocument/2006/math"
  elementFormDefault="qualified">
  <xsd:complexType name="CT_OMath"/>
  <xsd:element name="oMath" type="CT_OMath"/>
</xsd:schema>"#;

/// Excerpts of `shared-relationshipReference.xsd`.
const RELATIONSHIPS: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns:s="http://schemas.openxmlformats.org/officeDocument/2006/sharedTypes"
  xmlns="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  targetNamespace="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  elementFormDefault="qualified">
  <xsd:attribute name="id" type="s:ST_RelationshipId"/>
</xsd:schema>"#;

/// Excerpts of `sml.xsd`.
const SML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
  targetNamespace="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
  elementFormDefault="qualified">
  <xsd:simpleType name="ST_Ref">
    <xsd:restriction base="xsd:string"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_Sqref">
    <xsd:list itemType="ST_Ref"/>
  </xsd:simpleType>
</xsd:schema>"#;

/// Excerpts of `dml-main.xsd`.
const DML: &str = r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns="http://schemas.openxmlformats.org/drawingml/2006/main"
  targetNamespace="http://schemas.openxmlformats.org/drawingml/2006/main"
  elementFormDefault="qualified">
  <xsd:complexType name="CT_GraphicalObjectData">
    <xsd:sequence>
      <xsd:any minOccurs="0" maxOccurs="unbounded" processContents="strict"/>
    </xsd:sequence>
    <xsd:attribute name="uri" type="xsd:token" use="required"/>
  </xsd:complexType>
  <xsd:complexType name="CT_GraphicalObject">
    <xsd:sequence>
      <xsd:element name="graphicData" type="CT_GraphicalObjectData"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:element name="graphic" type="CT_GraphicalObject"/>
</xsd:schema>"#;

const FILES: [(&str, &str); 6] = [
    ("schemas/dml-main.xsd", DML),
    ("schemas/shared-commonSimpleTypes.xsd", SHARED),
    ("schemas/shared-math.xsd", MATH),
    ("schemas/shared-relationshipReference.xsd", RELATIONSHIPS),
    ("schemas/sml.xsd", SML),
    ("schemas/wml.xsd", WML),
];

fn complex(name: &str) -> TypeRef {
    TypeRef::Complex {
        anonymous: false,
        name: Name::new(W, name),
    }
}

fn complex_type<'set>(set: &'set SchemaSet, namespace: &str, local: &str) -> &'set ComplexType {
    &set.complex_types[&Name::new(namespace, local)]
}

fn element(
    namespace: &str,
    local: &str,
    type_ref: TypeRef,
    min: u32,
    max: Option<u32>,
) -> Particle {
    Particle {
        kind: ParticleKind::Element {
            name: Name::new(namespace, local),
            type_ref,
        },
        max,
        min,
    }
}

fn fixture() -> SchemaSet {
    read(&FILES).unwrap()
}

fn local_names(attributes: &[AttributeUse]) -> Vec<&str> {
    attributes
        .iter()
        .map(|attribute| attribute.name.local.as_str())
        .collect()
}

fn read(files: &[(&str, &str)]) -> anyhow::Result<SchemaSet> {
    let mut system = MemorySystem::new();
    for (path, text) in files {
        system = system.with_file(format!("/{path}"), text.as_bytes())?;
    }
    let paths: Vec<&str> = files.iter().map(|(path, _)| *path).collect();
    SchemaSet::read(&system, &paths)
}

fn read_error(files: &[(&str, &str)]) -> String {
    format!("{:#}", read(files).unwrap_err())
}

const fn sequence(children: Vec<Particle>) -> Particle {
    Particle {
        kind: ParticleKind::Sequence(children),
        max: Some(1),
        min: 1,
    }
}

fn simple(namespace: &str, name: &str) -> TypeRef {
    TypeRef::Simple {
        anonymous: false,
        name: Name::new(namespace, name),
    }
}

#[test]
fn reads_every_file_into_its_namespace() {
    let set = fixture();
    let namespaces: Vec<&str> = set.namespaces.keys().map(String::as_str).collect();
    assert_eq!(namespaces, [A, M, R, S, X, W]);
    let wml = &set.namespaces[W];
    assert_eq!(wml.path, Path::new("schemas/wml.xsd"));
    assert_eq!(wml.imports, [M, S, XML]);
    assert_eq!(wml.prefixes.get("r"), Some(&R.to_owned()));
}

#[test]
fn resolves_a_group_reference_to_the_groups_choice() {
    let set = fixture();
    let block = &set.groups[&Name::new(W, "EG_BlockLevelElts")];
    assert_eq!(
        block.particle,
        Particle {
            kind: ParticleKind::Choice(vec![
                Particle {
                    kind: ParticleKind::GroupRef(Name::new(W, "EG_BlockLevelChunkElts")),
                    max: None,
                    min: 0,
                },
                element(W, "altChunk", complex("CT_AltChunk"), 0, None),
            ]),
            max: Some(1),
            min: 1,
        }
    );
    let chunk = &set.groups[&Name::new(W, "EG_BlockLevelChunkElts")];
    assert_eq!(
        chunk.particle,
        Particle {
            kind: ParticleKind::Choice(vec![element(W, "p", complex("CT_P"), 1, Some(1))]),
            max: Some(1),
            min: 1,
        }
    );
}

#[test]
fn expands_attribute_groups_in_declaration_order() {
    let set = fixture();
    let protect = complex_type(&set, W, "CT_DocProtect");
    assert_eq!(
        local_names(&protect.attributes),
        [
            "edit",
            "formatting",
            "enforcement",
            "algorithmName",
            "hashValue",
            "saltValue",
            "spinCount",
            "cryptProviderType",
            "cryptAlgorithmClass",
            "cryptAlgorithmType",
            "cryptAlgorithmSid",
            "cryptSpinCount",
            "cryptProvider",
            "algIdExt",
            "algIdExtSource",
            "cryptProviderTypeExt",
            "cryptProviderTypeExtSource",
            "hash",
            "salt",
        ]
    );
    let groups: Vec<Option<&str>> = protect
        .attributes
        .iter()
        .map(|attribute| attribute.group.as_ref().map(|group| group.local.as_str()))
        .collect();
    assert_eq!(groups[..3], [None, None, None]);
    assert_eq!(groups[3..7], [Some("AG_Password"); 4]);
    assert_eq!(groups[7..], [Some("AG_TransitionalPassword"); 12]);
    assert_eq!(
        protect.attributes[4],
        AttributeUse {
            default: None,
            fixed: None,
            group: Some(Name::new(W, "AG_Password")),
            name: Name::new(W, "hashValue"),
            required: false,
            type_ref: TypeRef::BuiltIn("base64Binary".to_owned()),
        }
    );
    let transitional = &set.attribute_groups[&Name::new(W, "AG_TransitionalPassword")];
    assert_eq!(transitional.attributes.len(), 12);
}

#[test]
fn keeps_a_derived_types_own_declarations_and_its_base() {
    let set = fixture();
    let comment = complex_type(&set, W, "CT_Comment");
    assert_eq!(local_names(&comment.attributes), ["initials"]);
    assert_eq!(
        comment.base,
        Some(Derivation {
            base: complex("CT_TrackChange"),
            method: Method::Extension,
        })
    );
}

#[test]
fn folds_in_an_extensions_base_attributes_first_and_its_particle_first() {
    let set = fixture();
    let comment = set.effective(complex_type(&set, W, "CT_Comment"));
    assert_eq!(
        local_names(&comment.attributes),
        ["id", "author", "date", "initials"]
    );
    assert_eq!(
        comment.content,
        Content::Particle(sequence(vec![Particle {
            kind: ParticleKind::GroupRef(Name::new(W, "EG_BlockLevelElts")),
            max: None,
            min: 0,
        }]))
    );
    let properties = set.effective(complex_type(&set, W, "CT_PPr"));
    assert_eq!(
        properties.content,
        Content::Particle(sequence(vec![
            sequence(vec![element(
                W,
                "keepNext",
                complex("CT_OnOff"),
                0,
                Some(1)
            )]),
            sequence(vec![element(W, "rPr", complex("CT_OnOff"), 0, Some(1))]),
        ]))
    );
}

#[test]
fn folds_in_a_restrictions_base_attributes_as_it_redeclares_them() {
    let set = fixture();
    let strict = set.effective(complex_type(&set, W, "CT_StrictOnOff"));
    assert_eq!(
        strict.attributes,
        [AttributeUse {
            default: None,
            fixed: None,
            group: None,
            name: Name::new(W, "val"),
            required: true,
            type_ref: simple(S, "ST_OnOff"),
        }]
    );
    assert_eq!(
        strict.content,
        Content::Particle(sequence(vec![element(
            W,
            "note",
            TypeRef::BuiltIn("string".to_owned()),
            1,
            Some(1),
        )]))
    );
}

#[test]
fn reads_a_paragraph_as_five_attributes_its_properties_and_its_content_group() {
    let set = fixture();
    let paragraph = complex_type(&set, W, "CT_P");
    assert_eq!(
        local_names(&paragraph.attributes),
        ["rsidRPr", "rsidR", "rsidDel", "rsidP", "rsidRDefault"]
    );
    assert_eq!(
        paragraph.content,
        Content::Particle(sequence(vec![
            element(W, "pPr", complex("CT_PPr"), 0, Some(1)),
            Particle {
                kind: ParticleKind::GroupRef(Name::new(W, "EG_PContent")),
                max: None,
                min: 0,
            },
        ]))
    );
}

#[test]
fn reads_the_cell_margins_down_to_the_union_of_their_width() {
    let set = fixture();
    let margins = complex_type(&set, W, "CT_TblCellMar");
    let children: Vec<Particle> = ["top", "start", "left", "bottom", "end", "right"]
        .into_iter()
        .map(|side| element(W, side, complex("CT_TblWidth"), 0, Some(1)))
        .collect();
    assert_eq!(margins.content, Content::Particle(sequence(children)));
    let width = complex_type(&set, W, "CT_TblWidth");
    assert_eq!(
        width.attributes[0].type_ref,
        simple(W, "ST_MeasurementOrPercent")
    );
    assert_eq!(
        set.simple_types[&Name::new(W, "ST_MeasurementOrPercent")],
        SimpleType::Union {
            members: vec![
                simple(W, "ST_DecimalNumberOrPercent"),
                simple(S, "ST_UniversalMeasure"),
            ],
            name: Name::new(W, "ST_MeasurementOrPercent"),
        }
    );
}

#[test]
fn reads_a_union_and_a_list() {
    let set = fixture();
    assert_eq!(
        set.simple_types[&Name::new(S, "ST_OnOff")],
        SimpleType::Union {
            members: vec![
                TypeRef::BuiltIn("boolean".to_owned()),
                simple(S, "ST_OnOff1"),
            ],
            name: Name::new(S, "ST_OnOff"),
        }
    );
    assert_eq!(
        set.simple_types[&Name::new(X, "ST_Sqref")],
        SimpleType::List {
            item: simple(X, "ST_Ref"),
            name: Name::new(X, "ST_Sqref"),
        }
    );
}

#[test]
fn reads_a_wildcard_with_its_namespace_and_process_contents() {
    let set = fixture();
    let data = complex_type(&set, A, "CT_GraphicalObjectData");
    let any = Particle {
        kind: ParticleKind::Any {
            namespace: "##any".to_owned(),
            process_contents: "strict".to_owned(),
        },
        max: None,
        min: 0,
    };
    assert_eq!(data.content, Content::Particle(sequence(vec![any.clone()])));
    assert_eq!(
        set.wildcards(),
        [(&Name::new(A, "CT_GraphicalObjectData"), &any)]
    );
}

#[test]
fn resolves_an_element_reference_across_namespaces_to_the_global_elements_type() {
    let set = fixture();
    let content = &set.groups[&Name::new(W, "EG_PContent")];
    assert_eq!(
        content.particle,
        Particle {
            kind: ParticleKind::Choice(vec![element(
                M,
                "oMath",
                TypeRef::Complex {
                    anonymous: false,
                    name: Name::new(M, "CT_OMath"),
                },
                1,
                Some(1),
            )]),
            max: Some(1),
            min: 1,
        }
    );
}

#[test]
fn resolves_an_attribute_reference_to_the_global_attributes_type() {
    let set = fixture();
    let chunk = complex_type(&set, W, "CT_AltChunk");
    assert_eq!(
        chunk.attributes,
        [AttributeUse {
            default: None,
            fixed: None,
            group: None,
            name: Name::new(R, "id"),
            required: false,
            type_ref: simple(S, "ST_RelationshipId"),
        }]
    );
    assert_eq!(
        set.attributes[&Name::new(R, "id")].type_ref,
        simple(S, "ST_RelationshipId")
    );
}

#[test]
fn keeps_references_outside_the_set_by_name() {
    let set = fixture();
    let text = complex_type(&set, W, "CT_Text");
    assert_eq!(text.attributes[0].name, Name::new(XML, "space"));
    assert_eq!(text.attributes[0].type_ref, TypeRef::Unknown);
    let picture = complex_type(&set, W, "CT_Picture");
    assert_eq!(
        picture.content,
        Content::Particle(sequence(vec![
            element(
                "urn:schemas-microsoft-com:vml",
                "shape",
                TypeRef::Unknown,
                1,
                Some(1)
            ),
            element(
                W,
                "extLst",
                TypeRef::Outside(Name::new("urn:schemas-microsoft-com:vml", "CT_Extension")),
                1,
                Some(1),
            ),
        ]))
    );
    assert!(picture.mixed);
}

#[test]
fn reads_simple_content_as_the_text_of_its_base() {
    let set = fixture();
    let text = complex_type(&set, W, "CT_Text");
    assert_eq!(text.content, Content::Simple(simple(S, "ST_String")));
    assert_eq!(
        text.base,
        Some(Derivation {
            base: simple(S, "ST_String"),
            method: Method::Extension,
        })
    );
    let short = complex_type(&set, W, "CT_ShortText");
    let restricted = TypeRef::Simple {
        anonymous: true,
        name: Name::new(W, "CT_ShortText/content"),
    };
    assert_eq!(short.content, Content::Simple(restricted));
    assert_eq!(
        set.simple_types[&Name::new(W, "CT_ShortText/content")],
        SimpleType::Restriction {
            base: simple(S, "ST_String"),
            enumeration: Vec::new(),
            facets: Facets {
                max_length: Some(8),
                ..Facets::default()
            },
            name: Name::new(W, "CT_ShortText/content"),
        }
    );
}

#[test]
fn names_an_anonymous_type_after_its_element() {
    let set = fixture();
    let document = &set.elements[&Name::new(W, "document")];
    assert_eq!(
        document,
        &Element {
            name: Name::new(W, "document"),
            type_ref: TypeRef::Complex {
                anonymous: true,
                name: Name::new(W, "/document"),
            },
        }
    );
    assert_eq!(
        complex_type(&set, W, "/document").content,
        Content::Particle(sequence(vec![element(W, "p", complex("CT_P"), 1, Some(1))]))
    );
}

#[test]
fn counts_tags_and_distinct_pairs_of_tag_and_type() {
    let set = fixture();
    let declarations = set.declarations();
    assert_eq!(
        declarations,
        Declarations {
            attributes: 36,
            element_references: 2,
            elements: 18,
        }
    );
    let pairs = set.element_tags();
    assert!(pairs.contains(&(&Name::new(W, "p"), &complex("CT_P"))));
    assert!(pairs.contains(&(
        &Name::new(M, "oMath"),
        &TypeRef::Complex {
            anonymous: false,
            name: Name::new(M, "CT_OMath"),
        }
    )));
    assert_eq!(pairs.len(), 18);
}

#[test]
fn refuses_a_reference_that_resolves_to_nothing() {
    let error = read_error(&[(
        "w.xsd",
        r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns="urn:w" targetNamespace="urn:w">
  <xsd:element name="e" type="CT_Missing"/>
</xsd:schema>"#,
    )]);
    assert!(
        error.contains(&format!("no type {}", Name::new("urn:w", "CT_Missing"))),
        "{error}"
    );
}

#[test]
fn refuses_an_unknown_built_in() {
    let error = read_error(&[(
        "w.xsd",
        r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:w">
  <xsd:element name="e" type="xsd:strng"/>
</xsd:schema>"#,
    )]);
    assert!(
        error.contains("is not an XML Schema built-in type"),
        "{error}"
    );
}

#[test]
fn refuses_two_files_with_one_target_namespace() {
    let schema =
        r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:w"/>"#;
    let error = read_error(&[("a.xsd", schema), ("b.xsd", schema)]);
    assert!(
        error.contains("which another file of the set declares"),
        "{error}"
    );
}

#[test]
fn refuses_a_definition_declared_twice() {
    let error = read_error(&[(
        "w.xsd",
        r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:w">
  <xsd:complexType name="CT_Twice"/>
  <xsd:complexType name="CT_Twice"/>
</xsd:schema>"#,
    )]);
    assert!(
        error.contains(&format!(
            "{} is declared twice",
            Name::new("urn:w", "CT_Twice")
        )),
        "{error}"
    );
}

#[test]
fn refuses_an_attribute_group_that_refers_to_itself() {
    let error = read_error(&[(
        "w.xsd",
        r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns="urn:w" targetNamespace="urn:w">
  <xsd:attributeGroup name="AG_Loop">
    <xsd:attributeGroup ref="AG_Loop"/>
  </xsd:attributeGroup>
</xsd:schema>"#,
    )]);
    assert!(
        error.contains(&format!(
            "the attribute group {} refers to itself",
            Name::new("urn:w", "AG_Loop")
        )),
        "{error}"
    );
}

#[test]
fn refuses_bases_that_form_a_cycle() {
    let error = read_error(&[(
        "w.xsd",
        r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns="urn:w" targetNamespace="urn:w">
  <xsd:complexType name="CT_A">
    <xsd:complexContent><xsd:extension base="CT_B"/></xsd:complexContent>
  </xsd:complexType>
  <xsd:complexType name="CT_B">
    <xsd:complexContent><xsd:extension base="CT_A"/></xsd:complexContent>
  </xsd:complexType>
</xsd:schema>"#,
    )]);
    assert!(error.contains("form a cycle"), "{error}");
}
