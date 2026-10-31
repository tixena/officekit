use std::path::Path;

use os_shim::mock::MemorySystem;

use super::{
    AttributeDecl, AttributeItem, ComplexDecl, ContentDecl, Schema, SimpleDecl, Term, TermKind,
    TypeName, Variety,
};
use crate::generate_schema::model::{Declarations, Facets, Method, Name, XSD_NAMESPACE};

const M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
const S: &str = "http://schemas.openxmlformats.org/officeDocument/2006/sharedTypes";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const XML: &str = "http://www.w3.org/XML/1998/namespace";

fn attribute(local: &str, type_name: TypeName, required: bool) -> AttributeItem {
    AttributeItem::Declaration(AttributeDecl {
        default: None,
        fixed: None,
        name: Name::new(W, local),
        required,
        type_name: Some(type_name),
    })
}

fn complex<'schema>(schema: &'schema Schema, local: &str) -> &'schema ComplexDecl {
    schema
        .complex_types
        .iter()
        .find(|decl| decl.name.local == local)
        .unwrap()
}

/// Element content of one particle, not derived.
fn content(particle: Term) -> ContentDecl {
    ContentDecl::Complex {
        derivation: None,
        particle: Some(particle),
    }
}

fn element(local: &str, type_name: TypeName, min: u32, max: Option<u32>) -> Term {
    term(
        TermKind::Element {
            name: Name::new(W, local),
            type_name,
        },
        min,
        max,
    )
}

fn named(namespace: &str, local: &str) -> TypeName {
    TypeName {
        anonymous: false,
        name: Name::new(namespace, local),
    }
}

fn read(text: &str) -> Schema {
    let system = MemorySystem::new()
        .with_file("/wml.xsd", text.as_bytes())
        .unwrap();
    Schema::read(&system, Path::new("wml.xsd")).unwrap()
}

fn read_error(text: &str) -> String {
    let system = MemorySystem::new()
        .with_file("/wml.xsd", text.as_bytes())
        .unwrap();
    format!(
        "{:#}",
        Schema::read(&system, Path::new("wml.xsd")).unwrap_err()
    )
}

fn sequence(children: Vec<Term>) -> Term {
    term(TermKind::Sequence(children), 1, Some(1))
}

fn simple<'schema>(schema: &'schema Schema, local: &str) -> &'schema SimpleDecl {
    schema
        .simple_types
        .iter()
        .find(|decl| decl.name.local == local)
        .unwrap()
}

const fn term(kind: TermKind, min: u32, max: Option<u32>) -> Term {
    Term { kind, max, min }
}

/// A `wml.xsd` header around `body`, with the prefixes and forms of the real file.
fn wml(body: &str) -> String {
    format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema"
  xmlns:m="{M}" xmlns:s="{S}" xmlns="{W}"
  elementFormDefault="qualified" attributeFormDefault="qualified" blockDefault="#all"
  targetNamespace="{W}">
  <xsd:import namespace="{M}" schemaLocation="shared-math.xsd"/>
  <xsd:import namespace="{S}" schemaLocation="shared-commonSimpleTypes.xsd"/>
  <xsd:import namespace="http://www.w3.org/XML/1998/namespace"/>
{body}
</xsd:schema>"##
    )
}

#[test]
fn reads_the_namespace_prefixes_imports_and_tag_counts() {
    let schema = read(&wml(r#"  <xsd:element name="document" type="CT_Document"/>
  <xsd:complexType name="CT_Document">
    <xsd:sequence>
      <xsd:element name="body" type="CT_Body" minOccurs="0"/>
      <xsd:element ref="m:oMath"/>
    </xsd:sequence>
    <xsd:attribute name="conformance" type="s:ST_ConformanceClass"/>
    <xsd:attribute ref="xml:space"/>
  </xsd:complexType>"#));
    assert_eq!(schema.namespace, W);
    assert_eq!(schema.imports, [M, S, XML]);
    assert_eq!(schema.prefixes.get(""), Some(&W.to_owned()));
    assert_eq!(schema.prefixes.get("s"), Some(&S.to_owned()));
    assert_eq!(schema.prefixes.get("xml"), None);
    assert_eq!(
        schema.declarations,
        Declarations {
            attributes: 2,
            element_references: 1,
            elements: 2,
        }
    );
}

#[test]
fn reads_a_global_element_and_resolves_its_type_through_the_default_namespace() {
    let schema = read(&wml(
        r#"  <xsd:element name="document" type="CT_Document"/>"#,
    ));
    assert_eq!(schema.elements.len(), 1);
    assert_eq!(schema.elements[0].name, Name::new(W, "document"));
    assert_eq!(schema.elements[0].type_name, named(W, "CT_Document"));
}

#[test]
fn resolves_a_prefixed_type_through_the_files_declarations() {
    let schema = read(&wml(r#"  <xsd:simpleType name="ST_MeasurementOrPercent">
    <xsd:union memberTypes="ST_DecimalNumberOrPercent s:ST_UniversalMeasure"/>
  </xsd:simpleType>"#));
    assert_eq!(
        simple(&schema, "ST_MeasurementOrPercent").variety,
        Variety::Union(vec![
            named(W, "ST_DecimalNumberOrPercent"),
            named(S, "ST_UniversalMeasure"),
        ])
    );
}

#[test]
fn reads_local_elements_and_a_group_reference_in_order_with_their_occurrences() {
    let schema = read(&wml(r#"  <xsd:complexType name="CT_P">
    <xsd:sequence>
      <xsd:element name="pPr" type="CT_PPr" minOccurs="0"/>
      <xsd:group ref="EG_PContent" minOccurs="0" maxOccurs="unbounded"/>
    </xsd:sequence>
    <xsd:attribute name="rsidRPr" type="ST_LongHexNumber"/>
    <xsd:attribute name="rsidR" type="ST_LongHexNumber"/>
  </xsd:complexType>"#));
    let paragraph = complex(&schema, "CT_P");
    assert_eq!(
        paragraph.content,
        content(sequence(vec![
            element("pPr", named(W, "CT_PPr"), 0, Some(1)),
            term(TermKind::GroupRef(Name::new(W, "EG_PContent")), 0, None),
        ]))
    );
    assert_eq!(
        paragraph.attributes,
        [
            attribute("rsidRPr", named(W, "ST_LongHexNumber"), false),
            attribute("rsidR", named(W, "ST_LongHexNumber"), false),
        ]
    );
}

#[test]
fn reads_an_element_reference_into_another_namespace() {
    let schema = read(&wml(r#"  <xsd:group name="EG_MathContent">
    <xsd:choice>
      <xsd:element ref="m:oMathPara"/>
      <xsd:element ref="m:oMath"/>
    </xsd:choice>
  </xsd:group>"#));
    assert_eq!(schema.groups[0].name, Name::new(W, "EG_MathContent"));
    assert_eq!(
        schema.groups[0].particle,
        term(
            TermKind::Choice(vec![
                term(TermKind::ElementRef(Name::new(M, "oMathPara")), 1, Some(1)),
                term(TermKind::ElementRef(Name::new(M, "oMath")), 1, Some(1)),
            ]),
            1,
            Some(1),
        )
    );
}

#[test]
fn reads_a_group_that_refers_to_another_group() {
    let schema = read(&wml(r#"  <xsd:group name="EG_BlockLevelChunkElts">
    <xsd:choice>
      <xsd:group ref="EG_ContentBlockContent" minOccurs="0" maxOccurs="unbounded"/>
    </xsd:choice>
  </xsd:group>
  <xsd:group name="EG_BlockLevelElts">
    <xsd:choice>
      <xsd:group ref="EG_BlockLevelChunkElts" minOccurs="0" maxOccurs="unbounded"/>
      <xsd:element name="altChunk" type="CT_AltChunk" minOccurs="0" maxOccurs="unbounded"/>
    </xsd:choice>
  </xsd:group>"#));
    assert_eq!(schema.groups[1].name, Name::new(W, "EG_BlockLevelElts"));
    assert_eq!(
        schema.groups[1].particle,
        term(
            TermKind::Choice(vec![
                term(
                    TermKind::GroupRef(Name::new(W, "EG_BlockLevelChunkElts")),
                    0,
                    None,
                ),
                element("altChunk", named(W, "CT_AltChunk"), 0, None),
            ]),
            1,
            Some(1),
        )
    );
}

#[test]
fn reads_attribute_groups_and_their_references_in_declaration_order() {
    let schema = read(&wml(r#"  <xsd:attributeGroup name="AG_Password">
    <xsd:attribute name="algorithmName" type="s:ST_String" use="optional"/>
  </xsd:attributeGroup>
  <xsd:complexType name="CT_DocProtect">
    <xsd:attribute name="edit" type="ST_DocProtect" use="optional"/>
    <xsd:attributeGroup ref="AG_Password"/>
    <xsd:attribute name="enforcement" type="s:ST_OnOff" use="required"/>
  </xsd:complexType>"#));
    assert_eq!(schema.attribute_groups[0].name, Name::new(W, "AG_Password"));
    assert_eq!(
        schema.attribute_groups[0].attributes,
        [attribute("algorithmName", named(S, "ST_String"), false)]
    );
    assert_eq!(
        complex(&schema, "CT_DocProtect").attributes,
        [
            attribute("edit", named(W, "ST_DocProtect"), false),
            AttributeItem::Group(Name::new(W, "AG_Password")),
            attribute("enforcement", named(S, "ST_OnOff"), true),
        ]
    );
}

#[test]
fn reads_complex_content_extension_and_restriction() {
    let schema = read(&wml(r#"  <xsd:complexType name="CT_TrackChange">
    <xsd:complexContent>
      <xsd:extension base="CT_Markup">
        <xsd:attribute name="author" type="s:ST_String" use="required"/>
      </xsd:extension>
    </xsd:complexContent>
  </xsd:complexType>
  <xsd:complexType name="CT_Narrow">
    <xsd:complexContent mixed="true">
      <xsd:restriction base="CT_Wide">
        <xsd:sequence>
          <xsd:element name="only" type="xsd:string"/>
        </xsd:sequence>
      </xsd:restriction>
    </xsd:complexContent>
  </xsd:complexType>"#));
    let change = complex(&schema, "CT_TrackChange");
    assert_eq!(
        change.content,
        ContentDecl::Complex {
            derivation: Some((Method::Extension, Name::new(W, "CT_Markup"))),
            particle: None,
        }
    );
    assert_eq!(
        change.attributes,
        [attribute("author", named(S, "ST_String"), true)]
    );
    let narrow = complex(&schema, "CT_Narrow");
    assert!(narrow.mixed);
    assert_eq!(
        narrow.content,
        ContentDecl::Complex {
            derivation: Some((Method::Restriction, Name::new(W, "CT_Wide"))),
            particle: Some(sequence(vec![element(
                "only",
                named(XSD_NAMESPACE, "string"),
                1,
                Some(1),
            )])),
        }
    );
}

#[test]
fn reads_simple_content_with_an_attribute_reference() {
    let schema = read(&wml(r#"  <xsd:complexType name="CT_Text">
    <xsd:simpleContent>
      <xsd:extension base="s:ST_String">
        <xsd:attribute ref="xml:space" use="optional"/>
      </xsd:extension>
    </xsd:simpleContent>
  </xsd:complexType>"#));
    let text = complex(&schema, "CT_Text");
    assert_eq!(
        text.content,
        ContentDecl::Simple {
            base: Name::new(S, "ST_String"),
            method: Method::Extension,
            restricted: None,
        }
    );
    assert_eq!(
        text.attributes,
        [AttributeItem::Declaration(AttributeDecl {
            default: None,
            fixed: None,
            name: Name::new(XML, "space"),
            required: false,
            type_name: None,
        })]
    );
}

#[test]
fn hoists_the_facets_of_a_simple_content_restriction() {
    let schema = read(&wml(r#"  <xsd:complexType name="CT_Short">
    <xsd:simpleContent>
      <xsd:restriction base="CT_Text">
        <xsd:maxLength value="8"/>
      </xsd:restriction>
    </xsd:simpleContent>
  </xsd:complexType>"#));
    assert_eq!(
        complex(&schema, "CT_Short").content,
        ContentDecl::Simple {
            base: Name::new(W, "CT_Text"),
            method: Method::Restriction,
            restricted: Some(Name::new(W, "CT_Short/content")),
        }
    );
    assert_eq!(
        simple(&schema, "CT_Short/content").variety,
        Variety::Restriction {
            base: named(W, "CT_Text"),
            enumeration: Vec::new(),
            facets: Facets {
                max_length: Some(8),
                ..Facets::default()
            },
        }
    );
}

#[test]
fn reads_an_all_group() {
    let schema = read(&wml(r#"  <xsd:complexType name="CT_Properties">
    <xsd:all>
      <xsd:element name="Template" minOccurs="0" maxOccurs="1" type="xsd:string"/>
      <xsd:element name="Pages" minOccurs="0" maxOccurs="1" type="xsd:int"/>
    </xsd:all>
  </xsd:complexType>"#));
    assert_eq!(
        complex(&schema, "CT_Properties").content,
        content(term(
            TermKind::All(vec![
                element("Template", named(XSD_NAMESPACE, "string"), 0, Some(1)),
                element("Pages", named(XSD_NAMESPACE, "int"), 0, Some(1)),
            ]),
            1,
            Some(1),
        ))
    );
}

#[test]
fn reads_wildcards_with_their_namespace_and_process_contents() {
    let schema = read(&wml(r#"  <xsd:complexType name="CT_GraphicalObjectData">
    <xsd:sequence>
      <xsd:any minOccurs="0" maxOccurs="unbounded" processContents="strict"/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Background">
    <xsd:sequence maxOccurs="unbounded">
      <xsd:any processContents="lax" namespace="urn:schemas-microsoft-com:vml" minOccurs="0"
        maxOccurs="unbounded"/>
      <xsd:any/>
    </xsd:sequence>
  </xsd:complexType>"#));
    assert_eq!(
        complex(&schema, "CT_GraphicalObjectData").content,
        content(sequence(vec![term(
            TermKind::Any {
                namespace: "##any".to_owned(),
                process_contents: "strict".to_owned(),
            },
            0,
            None,
        )]))
    );
    assert_eq!(
        complex(&schema, "CT_Background").content,
        content(term(
            TermKind::Sequence(vec![
                term(
                    TermKind::Any {
                        namespace: "urn:schemas-microsoft-com:vml".to_owned(),
                        process_contents: "lax".to_owned(),
                    },
                    0,
                    None,
                ),
                term(
                    TermKind::Any {
                        namespace: "##any".to_owned(),
                        process_contents: "strict".to_owned(),
                    },
                    1,
                    Some(1),
                ),
            ]),
            1,
            None,
        ))
    );
}

#[test]
fn reads_every_restriction_facet() {
    let schema = read(&wml(r#"  <xsd:simpleType name="ST_Jc">
    <xsd:restriction base="xsd:string">
      <xsd:enumeration value="start"/>
      <xsd:enumeration value="left"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_LongHexNumber">
    <xsd:restriction base="xsd:hexBinary">
      <xsd:length value="4"/>
    </xsd:restriction>
  </xsd:simpleType>
  <xsd:simpleType name="ST_Everything">
    <xsd:restriction base="xsd:decimal">
      <xsd:pattern value="-?[0-9]+(\.[0-9]+)?(mm|cm)"/>
      <xsd:minLength value="1"/>
      <xsd:maxLength value="255"/>
      <xsd:minInclusive value="-27273042329600"/>
      <xsd:maxInclusive value="600"/>
      <xsd:minExclusive value="-1"/>
      <xsd:maxExclusive value="601"/>
    </xsd:restriction>
  </xsd:simpleType>"#));
    assert_eq!(
        simple(&schema, "ST_Jc").variety,
        Variety::Restriction {
            base: named(XSD_NAMESPACE, "string"),
            enumeration: vec!["start".to_owned(), "left".to_owned()],
            facets: Facets::default(),
        }
    );
    assert_eq!(
        simple(&schema, "ST_LongHexNumber").variety,
        Variety::Restriction {
            base: named(XSD_NAMESPACE, "hexBinary"),
            enumeration: Vec::new(),
            facets: Facets {
                length: Some(4),
                ..Facets::default()
            },
        }
    );
    assert_eq!(
        simple(&schema, "ST_Everything").variety,
        Variety::Restriction {
            base: named(XSD_NAMESPACE, "decimal"),
            enumeration: Vec::new(),
            facets: Facets {
                length: None,
                max_exclusive: Some("601".to_owned()),
                max_inclusive: Some("600".to_owned()),
                max_length: Some(255),
                min_exclusive: Some("-1".to_owned()),
                min_inclusive: Some("-27273042329600".to_owned()),
                min_length: Some(1),
                patterns: vec![r"-?[0-9]+(\.[0-9]+)?(mm|cm)".to_owned()],
            },
        }
    );
}

#[test]
fn reads_a_list_and_a_union_with_inline_members() {
    let schema = read(&wml(r#"  <xsd:simpleType name="ST_Sqref">
    <xsd:list itemType="ST_Ref"/>
  </xsd:simpleType>
  <xsd:simpleType name="ST_TextRotation">
    <xsd:union>
      <xsd:simpleType>
        <xsd:restriction base="xsd:nonNegativeInteger">
          <xsd:maxInclusive value="180"/>
        </xsd:restriction>
      </xsd:simpleType>
      <xsd:simpleType>
        <xsd:restriction base="xsd:nonNegativeInteger">
          <xsd:enumeration value="255"/>
        </xsd:restriction>
      </xsd:simpleType>
    </xsd:union>
  </xsd:simpleType>"#));
    assert_eq!(
        simple(&schema, "ST_Sqref").variety,
        Variety::List(named(W, "ST_Ref"))
    );
    assert_eq!(
        simple(&schema, "ST_TextRotation").variety,
        Variety::Union(vec![
            TypeName {
                anonymous: true,
                name: Name::new(W, "ST_TextRotation/1"),
            },
            TypeName {
                anonymous: true,
                name: Name::new(W, "ST_TextRotation/2"),
            },
        ])
    );
    assert_eq!(
        simple(&schema, "ST_TextRotation/2").variety,
        Variety::Restriction {
            base: named(XSD_NAMESPACE, "nonNegativeInteger"),
            enumeration: vec!["255".to_owned()],
            facets: Facets::default(),
        }
    );
}

#[test]
fn hoists_anonymous_types_under_names_derived_from_their_element() {
    let schema = read(&wml(r#"  <xsd:element name="root">
    <xsd:complexType>
      <xsd:sequence>
        <xsd:element name="item" maxOccurs="unbounded">
          <xsd:simpleType>
            <xsd:restriction base="xsd:string"/>
          </xsd:simpleType>
        </xsd:element>
        <xsd:element name="untyped"/>
      </xsd:sequence>
      <xsd:attribute name="kind">
        <xsd:simpleType>
          <xsd:restriction base="xsd:token"/>
        </xsd:simpleType>
      </xsd:attribute>
    </xsd:complexType>
  </xsd:element>"#));
    assert_eq!(
        schema.elements[0].type_name,
        TypeName {
            anonymous: true,
            name: Name::new(W, "/root"),
        }
    );
    let root = complex(&schema, "/root");
    assert_eq!(
        root.content,
        content(sequence(vec![
            element(
                "item",
                TypeName {
                    anonymous: true,
                    name: Name::new(W, "/root/item"),
                },
                1,
                None,
            ),
            element("untyped", named(XSD_NAMESPACE, "anyType"), 1, Some(1)),
        ]))
    );
    assert_eq!(
        root.attributes,
        [attribute(
            "kind",
            TypeName {
                anonymous: true,
                name: Name::new(W, "/root/@kind"),
            },
            false,
        )]
    );
    assert_eq!(
        simple(&schema, "/root/item").variety,
        Variety::Restriction {
            base: named(XSD_NAMESPACE, "string"),
            enumeration: Vec::new(),
            facets: Facets::default(),
        }
    );
}

#[test]
fn names_a_second_anonymous_type_of_one_path_apart() {
    let schema = read(&wml(r#"  <xsd:complexType name="CT_Twice">
    <xsd:choice>
      <xsd:element name="x"><xsd:complexType/></xsd:element>
      <xsd:element name="x"><xsd:complexType/></xsd:element>
    </xsd:choice>
  </xsd:complexType>"#));
    let names: Vec<&str> = schema
        .complex_types
        .iter()
        .map(|decl| decl.name.local.as_str())
        .collect();
    assert_eq!(names, ["CT_Twice/x", "CT_Twice/x#2", "CT_Twice"]);
}

#[test]
fn reads_mixed_content() {
    let schema = read(&wml(r#"  <xsd:complexType name="CT_Schema" mixed="true">
    <xsd:sequence>
      <xsd:any/>
    </xsd:sequence>
  </xsd:complexType>
  <xsd:complexType name="CT_Plain"/>"#));
    assert!(complex(&schema, "CT_Schema").mixed);
    assert!(!complex(&schema, "CT_Plain").mixed);
}

#[test]
fn qualifies_local_names_by_the_forms_of_the_file() {
    let schema = read(&format!(
        r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" targetNamespace="{W}">
  <xsd:attribute name="global" type="xsd:string"/>
  <xsd:complexType name="CT_Forms">
    <xsd:sequence>
      <xsd:element name="plain" type="xsd:string"/>
      <xsd:element name="qualified" type="xsd:string" form="qualified"/>
    </xsd:sequence>
    <xsd:attribute name="plain" type="xsd:string"/>
    <xsd:attribute name="qualified" type="xsd:string" form="qualified"/>
  </xsd:complexType>
</xsd:schema>"#
    ));
    assert_eq!(schema.attributes[0].name, Name::new(W, "global"));
    let forms = complex(&schema, "CT_Forms");
    assert_eq!(
        forms.content,
        content(sequence(vec![
            term(
                TermKind::Element {
                    name: Name::new("", "plain"),
                    type_name: named(XSD_NAMESPACE, "string"),
                },
                1,
                Some(1),
            ),
            element("qualified", named(XSD_NAMESPACE, "string"), 1, Some(1)),
        ]))
    );
    assert_eq!(
        forms.attributes,
        [
            AttributeItem::Declaration(AttributeDecl {
                default: None,
                fixed: None,
                name: Name::new("", "plain"),
                required: false,
                type_name: Some(named(XSD_NAMESPACE, "string")),
            }),
            attribute("qualified", named(XSD_NAMESPACE, "string"), false),
        ]
    );
}

#[test]
fn reads_a_byte_order_mark_and_an_xs_prefix() {
    let schema = read(
        "\u{feff}<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" \
         targetNamespace=\"urn:t\"><xs:element name=\"e\" type=\"xs:string\"/></xs:schema>",
    );
    assert_eq!(schema.elements[0].name, Name::new("urn:t", "e"));
    assert_eq!(schema.elements[0].type_name, named(XSD_NAMESPACE, "string"));
}

#[test]
fn refuses_an_undeclared_prefix() {
    let error = read_error(&wml(r#"  <xsd:element name="e" type="q:CT_E"/>"#));
    assert!(
        error.contains("the prefix of q:CT_E is not declared"),
        "{error}"
    );
}

#[test]
fn refuses_a_construct_it_does_not_model() {
    let error = read_error(&wml(r#"  <xsd:complexType name="CT_Open">
    <xsd:anyAttribute/>
  </xsd:complexType>"#));
    assert!(
        error.contains("xsd:anyAttribute is not modelled in a complex type"),
        "{error}"
    );
}

#[test]
fn refuses_a_root_other_than_schema() {
    let error = read_error("<root/>");
    assert!(
        error.contains("the root element is not xsd:schema"),
        "{error}"
    );
}

#[test]
fn refuses_identity_constraints_prohibited_attributes_and_substitution_groups() {
    let key = read_error(&wml(r#"  <xsd:element name="e" type="CT_E">
    <xsd:key name="k"><xsd:selector xpath="."/><xsd:field xpath="@id"/></xsd:key>
  </xsd:element>"#));
    assert!(
        key.contains("xsd:key is not modelled beside type="),
        "{key}"
    );
    let prohibited = read_error(&wml(r#"  <xsd:complexType name="CT_E">
    <xsd:attribute name="a" type="xsd:string" use="prohibited"/>
  </xsd:complexType>"#));
    assert!(
        prohibited.contains("use=\"prohibited\" is not modelled"),
        "{prohibited}"
    );
    let substitution = read_error(&wml(
        r#"  <xsd:element name="e" type="CT_E" substitutionGroup="f"/>"#,
    ));
    assert!(
        substitution.contains("substitution groups are not modelled"),
        "{substitution}"
    );
}
