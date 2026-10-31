use std::path::Path;

use os_shim::mock::MemorySystem;

use super::{DataChildKind, DataVersion, Extension, ExtensionContent, Occurs, SdkData};

const NAMESPACES: &str = r#"[
  {"Prefix": "", "Uri": ""},
  {"Prefix": "w", "Uri": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"},
  {"Prefix": "w14", "Uri": "http://schemas.microsoft.com/office/word/2010/wordml", "Version": "Office2010"},
  {"Prefix": "w16sdtfl", "Uri": "http://schemas.microsoft.com/office/word/2024/wordml/sdtformatlock", "Version": "Microsoft365"},
  {"Prefix": "xvml", "Uri": "urn:schemas-microsoft-com:office:excel"}
]"#;

const TYPED_NAMESPACES: &str = r#"[
  {"Prefix": "w", "Namespace": "DocumentFormat.OpenXml.Wordprocessing"},
  {"Prefix": "w14", "Namespace": "DocumentFormat.OpenXml.Office2010.Word"}
]"#;

/// The two `w:w` rows of blueprint section 3.6, whole, and `w:p`'s and `w:rPr`'s extension content.
const WORDPROCESSING: &str = r#"{
  "TargetNamespace": "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
  "Types": [
    {"Name": "w:CT_TblWidthDxaNil/", "ClassName": "TableWidthDxaNilType", "Attributes": [
      {"QName": "w:w", "PropertyName": "Width", "Type": "Int16Value", "PropertyComments": "w", "Validators": [{"Name": "RequiredValidator"}, {"Arguments": [{"Type": "Long", "Name": "MinInclusive", "Value": "0"}], "Name": "NumberValidator", "IsInitialVersion": true}]},
      {"QName": "w:type", "PropertyName": "Type", "Type": "EnumValue<DocumentFormat.OpenXml.Wordprocessing.TableWidthValues>", "Validators": [{"Name": "RequiredValidator"}]}
    ]},
    {"Name": "w:CT_TblWidth/", "ClassName": "TableWidthType", "Attributes": [
      {"QName": "w:w", "PropertyName": "Width", "Type": "StringValue", "PropertyComments": "Table Width Value", "Validators": [{"Name": "NumberValidator", "Type": "w:ST_DecimalNumber", "Version": "Office2007"}, {"Arguments": [{"Type": "String", "Name": "Pattern", "Value": "-?[0-9]+(\\.[0-9]+)?%"}], "Name": "StringValidator", "Version": "Office2010", "UnionId": 0}, {"Name": "NumberValidator", "Type": "w:ST_DecimalNumber", "Version": "Office2010", "UnionId": 0}, {"Arguments": [{"Type": "String", "Name": "Pattern", "Value": "-?[0-9]+(\\.[0-9]+)?%"}], "Name": "StringValidator", "Version": "Office2013", "UnionId": 1, "IsInitialVersion": true}, {"Name": "NumberValidator", "Type": "w:ST_DecimalNumber", "Version": "Office2013", "UnionId": 1, "IsInitialVersion": true}]},
      {"QName": "w:type", "PropertyName": "Type", "Type": "EnumValue<DocumentFormat.OpenXml.Wordprocessing.TableWidthUnitValues>"}
    ]},
    {"Name": "w:CT_P/w:p", "ClassName": "Paragraph", "Attributes": [
      {"QName": "w:rsidR", "PropertyName": "RsidParagraphAddition", "Type": "HexBinaryValue", "Validators": [{"Arguments": [{"Type": "Long", "Name": "Length", "Value": "4"}], "Name": "StringValidator", "IsInitialVersion": true}]},
      {"QName": "w14:paraId", "PropertyName": "ParagraphId", "Type": "HexBinaryValue", "Version": "Office2010", "Validators": [{"Arguments": [{"Type": "Version", "Value": "Office2010"}], "Name": "OfficeVersionValidator"}, {"Arguments": [{"Type": "Long", "Name": "Length", "Value": "4"}], "Name": "StringValidator", "IsInitialVersion": true}]},
      {"QName": "w14:textId", "PropertyName": "TextId", "Type": "HexBinaryValue", "Version": "Office2010", "Validators": [{"Arguments": [{"Type": "Version", "Value": "Office2010"}], "Name": "OfficeVersionValidator"}, {"Arguments": [{"Type": "Long", "Name": "Length", "Value": "4"}], "Name": "StringValidator", "IsInitialVersion": true}]},
      {"QName": "w14:noSpellErr", "PropertyName": "NoSpellError", "Type": "OnOffValue", "Version": "Office2010", "Validators": [{"Arguments": [{"Type": "Version", "Value": "Office2010"}], "Name": "OfficeVersionValidator"}]}
    ], "Particle": {"Kind": "Sequence", "Items": [{"Name": "w:CT_PPr/w:pPr", "Occurs": [{"Max": 1}]}]}},
    {"Name": "w:CT_RPr/w:rPr", "Particle": {"Kind": "Sequence", "Items": [
      {"Kind": "Group", "Occurs": [{"Max": 1}], "Items": [{"Kind": "Sequence", "Items": [{"Name": "w:CT_OnOff/w:b", "Occurs": [{"Max": 1}]}]}]},
      {"Kind": "Group", "InitialVersion": "Office2010", "Occurs": [{"Max": 1}], "Items": [{"Kind": "Sequence", "Items": [
        {"Name": "w14:CT_Glow/w14:glow", "Occurs": [{"Max": 1}], "InitialVersion": "Office2010"},
        {"Name": "w14:CT_Ligatures/w14:ligatures", "Occurs": [{"Max": 1}], "InitialVersion": "Office2010"}
      ]}]}
    ]}}
  ],
  "Enums": [
    {"Type": "w:ST_TblWidth", "Name": "TableWidthUnitValues", "Facets": [{"Comments": "No Width", "Value": "nil", "Name": "Nil"}, {"Value": "pct", "Name": "Pct"}, {"Value": "dxa", "Name": "Dxa"}, {"Value": "auto", "Name": "Auto"}]},
    {"Type": "w:ST_CalendarType", "Name": "CalendarValues", "Facets": [{"Value": "gregorian", "Name": "Gregorian"}, {"Version": "Office2010", "Value": "umalqura", "Name": "Umalqura"}]}
  ]
}"#;

const WORDML_2010: &str = r#"{
  "TargetNamespace": "http://schemas.microsoft.com/office/word/2010/wordml",
  "Types": [
    {"Name": "w14:CT_Glow/w14:glow", "Version": "Office2010", "Attributes": [{"QName": "w14:rad", "PropertyName": "GlowRadius", "Type": "Int64Value", "Version": "Office2010"}]}
  ]
}"#;

const EXCEL_VML: &str = r###"{
  "TargetNamespace": "urn:schemas-microsoft-com:office:excel",
  "Types": [
    {"Name": "xvml:ST_TrueFalseBlank/xvml:MoveWithCells", "ClassName": "MoveWithCells", "BaseClass": "OpenXmlLeafTextElement", "IsDerived": true, "IsLeafText": true, "Validators": [{"Name": "EnumValidator", "Type": "xvml:ST_TrueFalseBlank", "IsInitialVersion": true}]},
    {"Name": "xvml:CT_Choices/", "Particle": {"Kind": "Choice", "RequireFilter": true, "InitialVersion": "Office2010", "Occurs": [{"Min": 1}], "Items": [
      {"Name": "w:CT_OnOff/w:b", "Occurs": [{"Max": 1}, {"Max": 2, "Version": "Office2010", "IncludeVersion": true}]},
      {"Kind": "Any", "Namespace": "##other"},
      {"Kind": "Any"}
    ]}}
  ]
}"###;

const COMMENTS_PART: &str = r#"{"Root": "w:CT_Comments", "Name": "WordprocessingCommentsPart", "Base": "OpenXmlPart", "ContentType": "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml", "RelationshipType": "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments", "Target": "comments", "RootElement": "comments", "Paths": {"General": "."}, "Children": [
  {"MaxOccursGreatThanOne": true, "ApiName": "ChartParts", "Name": "ChartPart", "HasFixedContent": true},
  {"MaxOccursGreatThanOne": true, "ApiName": "EmbeddedObjectParts", "Name": "EmbeddedObjectPart", "IsSpecialEmbeddedPart": true},
  {"MaxOccursGreatThanOne": true, "IsDataPartReference": true, "ApiName": "VideoReferenceRelationships", "Name": "VideoReferenceRelationship"},
  {"MinOccursIsNonZero": true, "ApiName": "MainDocumentPart", "Name": "MainDocumentPart"}
]}"#;

const IMAGE_PART: &str = r#"{"Name": "ImagePart", "Base": "OpenXmlPart", "RelationshipType": "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image", "Target": "image", "Extension": ".bin", "Paths": {"General": "../media"}}"#;

const SCHEMATRONS: &str = r#"[
  {"Context": "w:decimalSymbol", "Test": "matches(@w:val, \u0022.{1}\u0022)", "App": "All"},
  {"Context": "x:sheet", "Test": "@sheetId < 65535", "App": "Excel", "Version": "Office2010"}
]"#;

/// A validator's name, type, union, version, and `IsInitialVersion`.
type ValidatorRow<'data> = (
    &'data str,
    Option<&'data str>,
    Option<u8>,
    Option<DataVersion>,
    bool,
);

fn system_with(extra: &[(&str, &str)]) -> MemorySystem {
    let mut system = MemorySystem::new();
    let files = [
        ("data/namespaces.json", NAMESPACES),
        ("data/typed/namespaces.json", TYPED_NAMESPACES),
        (
            "data/schemas/schemas_openxmlformats_org_wordprocessingml_2006_main.json",
            WORDPROCESSING,
        ),
        (
            "data/schemas/schemas_microsoft_com_office_word_2010_wordml.json",
            WORDML_2010,
        ),
        (
            "data/schemas/urn_schemas-microsoft-com_office_excel.json",
            EXCEL_VML,
        ),
        ("data/schemas/README.md", "not a schema"),
        ("data/parts/WordprocessingCommentsPart.json", COMMENTS_PART),
        ("data/parts/ImagePart.json", IMAGE_PART),
        ("data/schematrons.json", SCHEMATRONS),
    ];
    for (path, text) in files.iter().chain(extra) {
        system = system
            .with_file(format!("/{path}"), text.as_bytes())
            .unwrap();
    }
    system
}

fn read() -> SdkData {
    SdkData::read(&system_with(&[]), Path::new("data")).unwrap()
}

fn read_with(extra: &[(&str, &str)]) -> anyhow::Result<SdkData> {
    SdkData::read(&system_with(extra), Path::new("data"))
}

fn named<'data>(
    extensions: &[Extension<'data>],
) -> Vec<(&'data str, &'data str, Option<DataVersion>)> {
    extensions
        .iter()
        .map(|extension| {
            let name = match extension.content {
                ExtensionContent::Attribute(attribute) => attribute.qname.as_str(),
                ExtensionContent::Element(element) => element.name.as_deref().unwrap_or_default(),
            };
            (extension.namespace, name, extension.version)
        })
        .collect()
}

#[test]
fn reads_namespaces_with_their_versions() {
    let data = read();
    let namespaces: Vec<(&str, &str, Option<DataVersion>)> = data
        .namespaces
        .iter()
        .map(|namespace| {
            (
                namespace.prefix.as_str(),
                namespace.uri.as_str(),
                namespace.version,
            )
        })
        .collect();
    assert_eq!(
        namespaces,
        [
            ("", "", None),
            (
                "w",
                "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
                None
            ),
            (
                "w14",
                "http://schemas.microsoft.com/office/word/2010/wordml",
                Some(DataVersion::Office2010)
            ),
            (
                "w16sdtfl",
                "http://schemas.microsoft.com/office/word/2024/wordml/sdtformatlock",
                Some(DataVersion::Microsoft365)
            ),
            ("xvml", "urn:schemas-microsoft-com:office:excel", None),
        ]
    );
}

#[test]
fn typed_namespaces_resolve_an_enum_value() {
    let data = read();
    assert_eq!(data.typed_namespaces.len(), 2);
    assert_eq!(
        data.typed_namespaces["w"],
        "DocumentFormat.OpenXml.Wordprocessing"
    );

    let width_type = &data.types["w:CT_TblWidth/"].attributes[1];
    let (namespace, name) = width_type
        .type_name
        .strip_prefix("EnumValue<")
        .and_then(|inner| inner.strip_suffix('>'))
        .and_then(|inner| inner.rsplit_once('.'))
        .unwrap();
    let resolved: Vec<&str> = data
        .enumerations
        .iter()
        .filter(|(simple_type, enumeration)| {
            enumeration.name == name
                && simple_type
                    .split_once(':')
                    .is_some_and(|(prefix, _)| data.typed_namespaces[prefix] == namespace)
        })
        .map(|(simple_type, _)| simple_type.as_str())
        .collect();
    assert_eq!(resolved, ["w:ST_TblWidth"]);
}

#[test]
fn versioned_validators_form_unions() {
    let data = read();
    let width = &data.types["w:CT_TblWidth/"].attributes[0];
    assert_eq!(width.qname, "w:w");
    assert_eq!(width.type_name, "StringValue");
    let validators: Vec<ValidatorRow<'_>> = width
        .validators
        .iter()
        .map(|validator| {
            (
                validator.name.as_str(),
                validator.type_name.as_deref(),
                validator.union_id,
                validator.version,
                validator.is_initial_version,
            )
        })
        .collect();
    assert_eq!(
        validators,
        [
            (
                "NumberValidator",
                Some("w:ST_DecimalNumber"),
                None,
                Some(DataVersion::Office2007),
                false
            ),
            (
                "StringValidator",
                None,
                Some(0),
                Some(DataVersion::Office2010),
                false
            ),
            (
                "NumberValidator",
                Some("w:ST_DecimalNumber"),
                Some(0),
                Some(DataVersion::Office2010),
                false
            ),
            (
                "StringValidator",
                None,
                Some(1),
                Some(DataVersion::Office2013),
                true
            ),
            (
                "NumberValidator",
                Some("w:ST_DecimalNumber"),
                Some(1),
                Some(DataVersion::Office2013),
                true
            ),
        ]
    );
    assert_eq!(
        width.validators[1].arguments,
        [(
            "String".to_owned(),
            Some("Pattern".to_owned()),
            r"-?[0-9]+(\.[0-9]+)?%".to_owned()
        )]
    );
}

#[test]
fn required_then_bounded_number() {
    let data = read();
    let width = data.types["w:CT_TblWidthDxaNil/"]
        .attributes
        .iter()
        .find(|attribute| attribute.qname == "w:w")
        .unwrap();
    assert_eq!(width.property_name, "Width");
    assert_eq!(width.type_name, "Int16Value");
    assert_eq!(width.version, None);
    assert_eq!(
        width
            .validators
            .iter()
            .map(|validator| validator.name.as_str())
            .collect::<Vec<_>>(),
        ["RequiredValidator", "NumberValidator"]
    );
    assert_eq!(width.validators[0].arguments.len(), 0);
    assert!(!width.validators[0].is_initial_version);
    assert_eq!(
        width.validators[1].arguments,
        [(
            "Long".to_owned(),
            Some("MinInclusive".to_owned()),
            "0".to_owned()
        )]
    );
    assert!(width.validators[1].is_initial_version);
}

#[test]
fn occurs_alternatives_and_particle_fields() {
    let data = read();
    let particle = data.types["xvml:CT_Choices/"].particle.as_ref().unwrap();
    assert_eq!(particle.kind.as_deref(), Some("Choice"));
    assert!(particle.require_filter);
    assert_eq!(particle.initial_version, Some(DataVersion::Office2010));
    assert_eq!(
        particle.occurs,
        [Occurs {
            include_version: false,
            max: 0,
            min: 1,
            version: None
        }]
    );

    let element = &particle.items[0];
    assert_eq!(element.kind, None);
    assert_eq!(element.name.as_deref(), Some("w:CT_OnOff/w:b"));
    assert_eq!(
        element.occurs,
        [
            Occurs {
                include_version: false,
                max: 1,
                min: 0,
                version: None
            },
            Occurs {
                include_version: true,
                max: 2,
                min: 0,
                version: Some(DataVersion::Office2010)
            },
        ]
    );

    let any = &particle.items[1];
    assert_eq!(any.kind.as_deref(), Some("Any"));
    assert_eq!(any.namespace.as_deref(), Some("##other"));
    assert_eq!(any.occurs.len(), 0);
    assert_eq!(particle.items[2].namespace, None);
}

#[test]
fn leaf_text_types_keep_their_own_validators() {
    let data = read();
    let leaf = &data.types["xvml:ST_TrueFalseBlank/xvml:MoveWithCells"];
    assert!(leaf.is_derived);
    assert!(leaf.is_leaf_text);
    assert_eq!(leaf.attributes.len(), 0);
    assert_eq!(leaf.particle, None);
    assert_eq!(leaf.validators.len(), 1);
    assert_eq!(leaf.validators[0].name, "EnumValidator");
    assert_eq!(
        leaf.validators[0].type_name.as_deref(),
        Some("xvml:ST_TrueFalseBlank")
    );

    let plain = &data.types["w:CT_TblWidth/"];
    assert!(!plain.is_derived);
    assert!(!plain.is_leaf_text);
    assert_eq!(
        data.types["w14:CT_Glow/w14:glow"].version,
        Some(DataVersion::Office2010)
    );
}

#[test]
fn enumerations_keep_each_member_and_its_version() {
    let data = read();
    assert_eq!(data.enumerations.len(), 2);
    let calendar = &data.enumerations["w:ST_CalendarType"];
    assert_eq!(calendar.name, "CalendarValues");
    assert_eq!(calendar.version, None);
    let facets: Vec<(&str, Option<DataVersion>)> = calendar
        .facets
        .iter()
        .map(|facet| (facet.value.as_str(), facet.version))
        .collect();
    assert_eq!(
        facets,
        [
            ("gregorian", None),
            ("umalqura", Some(DataVersion::Office2010))
        ]
    );
    assert_eq!(data.enumerations["w:ST_TblWidth"].facets.len(), 4);
}

#[test]
fn parts_and_their_child_entries() {
    let data = read();
    let names: Vec<&str> = data.parts.iter().map(|part| part.name.as_str()).collect();
    assert_eq!(names, ["ImagePart", "WordprocessingCommentsPart"]);

    let image = &data.parts[0];
    assert_eq!(image.content_type, None);
    assert_eq!(image.root, None);
    assert_eq!(image.children.len(), 0);

    let comments = &data.parts[1];
    assert_eq!(comments.root.as_deref(), Some("w:CT_Comments"));
    assert_eq!(
        comments.content_type.as_deref(),
        Some("application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml")
    );
    assert_eq!(
        comments.relationship_type.as_deref(),
        Some("http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments")
    );
    let children: Vec<(&str, bool, DataChildKind, bool, bool)> = comments
        .children
        .iter()
        .map(|child| {
            (
                child.name.as_str(),
                child.fixed_content,
                child.kind,
                child.many,
                child.required,
            )
        })
        .collect();
    assert_eq!(
        children,
        [
            ("ChartPart", true, DataChildKind::Ordinary, true, false),
            (
                "EmbeddedObjectPart",
                false,
                DataChildKind::SpecialEmbedded,
                true,
                false
            ),
            (
                "VideoReferenceRelationship",
                false,
                DataChildKind::DataPart,
                true,
                false
            ),
            (
                "MainDocumentPart",
                false,
                DataChildKind::Ordinary,
                false,
                true
            ),
        ]
    );
}

#[test]
fn schematrons_are_kept_as_written() {
    let data = read();
    assert_eq!(data.schematrons.len(), 2);
    assert_eq!(data.schematrons[0]["Context"], "w:decimalSymbol");
    assert_eq!(data.schematrons[1]["Version"], "Office2010");
}

#[test]
fn extension_content_in_ecma_types() {
    let data = read();
    assert_eq!(
        named(&data.extensions_in("w:CT_P/w:p")),
        [
            ("w14", "w14:paraId", Some(DataVersion::Office2010)),
            ("w14", "w14:textId", Some(DataVersion::Office2010)),
            ("w14", "w14:noSpellErr", Some(DataVersion::Office2010)),
        ]
    );
    let in_run_properties = data.extensions_in("w:CT_RPr/w:rPr");
    assert_eq!(
        named(&in_run_properties),
        [
            ("w14", "w14:CT_Glow/w14:glow", Some(DataVersion::Office2010)),
            (
                "w14",
                "w14:CT_Ligatures/w14:ligatures",
                Some(DataVersion::Office2010)
            ),
        ]
    );
    assert!(
        in_run_properties
            .iter()
            .all(|extension| extension.type_name == "w:CT_RPr/w:rPr")
    );
    assert_eq!(data.extensions_in("w14:CT_Glow/w14:glow").len(), 0);
    assert_eq!(data.extensions_in("w:CT_TblWidth/").len(), 0);
    assert_eq!(data.extensions_in("w:CT_Missing/w:missing").len(), 0);
}

#[test]
fn extension_namespaces_with_and_without_content() {
    let data = read();
    let (with, without) = data.extension_namespaces();
    let prefixes = |namespaces: &[&super::DataNamespace]| -> Vec<String> {
        namespaces
            .iter()
            .map(|namespace| namespace.prefix.clone())
            .collect()
    };
    assert_eq!(prefixes(&with), ["w14"]);
    assert_eq!(prefixes(&without), ["w16sdtfl"]);
}

#[test]
fn identical_duplicate_types_merge() {
    let copy = (
        "data/schemas/zz_copy.json",
        r#"{"Types": [{"Name": "w14:CT_Glow/w14:glow", "Version": "Office2010", "Attributes": [{"QName": "w14:rad", "PropertyName": "GlowRadius", "Type": "Int64Value", "Version": "Office2010"}]}]}"#,
    );
    assert_eq!(read_with(&[copy]).unwrap().types.len(), read().types.len());
}

#[test]
fn conflicting_duplicate_types_fail() {
    let conflict = (
        "data/schemas/zz_conflict.json",
        r#"{"Types": [{"Name": "w14:CT_Glow/w14:glow", "Version": "Office2013"}]}"#,
    );
    let error = format!("{:#}", read_with(&[conflict]).unwrap_err());
    assert!(
        error.contains("two different types under `w14:CT_Glow/w14:glow`"),
        "{error}"
    );
}

#[test]
fn unknown_versions_fail() {
    let unknown = (
        "data/schemas/zz_unknown.json",
        r#"{"Types": [{"Name": "w:CT_New/w:new", "Version": "Office2030"}]}"#,
    );
    let error = format!("{:#}", read_with(&[unknown]).unwrap_err());
    assert!(error.contains("in type `w:CT_New/w:new`"), "{error}");
    assert!(error.contains("`Office2030`"), "{error}");
}

#[test]
fn a_child_part_of_two_kinds_fails() {
    let both = (
        "data/parts/ZzPart.json",
        r#"{"Name": "ZzPart", "Children": [{"Name": "X", "IsDataPartReference": true, "IsSpecialEmbeddedPart": true}]}"#,
    );
    let error = format!("{:#}", read_with(&[both]).unwrap_err());
    assert!(
        error.contains("both `IsDataPartReference` and `IsSpecialEmbeddedPart`"),
        "{error}"
    );
}

#[test]
fn a_missing_file_fails_with_its_path() {
    let error = format!(
        "{:#}",
        SdkData::read(&MemorySystem::new(), Path::new("data")).unwrap_err()
    );
    assert!(error.contains("data/namespaces.json"), "{error}");
}
