use alloc::collections::BTreeMap;

use super::{Definition, Kind, constant_name, join, kinds};
use crate::generate_schema::sdk_data::{DataPart, SdkData};
use crate::generate_schema::standard_text::{PartDefinition, TransitionalPart};

const STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const TRANSITIONAL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const CORE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";

const DOCUMENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const MACRO_DOCUMENT: &str = "application/vnd.ms-word.document.macroEnabled.main+xml";
const SHEET_COMMENTS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";
const WORD_COMMENTS: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml";

fn definition(section: &str, title: &str, content_types: &[&str], strict: &str) -> PartDefinition {
    PartDefinition {
        content_types: content_types.iter().map(|&text| text.to_owned()).collect(),
        section: section.to_owned(),
        source_relationship: Some(strict.to_owned()),
        title: title.to_owned(),
        ..PartDefinition::default()
    }
}

fn restatement(section: &str, part_1: &str, transitional: &str) -> TransitionalPart {
    TransitionalPart {
        part_1_section: part_1.to_owned(),
        section: section.to_owned(),
        source_relationship: Some(transitional.to_owned()),
        ..TransitionalPart::default()
    }
}

fn data_part(name: &str, content_type: Option<&str>, relationship: Option<&str>) -> DataPart {
    DataPart {
        base: Some("OpenXmlPart".to_owned()),
        children: Vec::new(),
        content_type: content_type.map(str::to_owned),
        name: name.to_owned(),
        relationship_type: relationship.map(str::to_owned),
        root: None,
        root_element: None,
        version: None,
    }
}

fn kind(file: &str, name: &str, content_type: &str) -> Kind {
    Kind {
        content_type: content_type.to_owned(),
        file: file.to_owned(),
        name: name.to_owned(),
    }
}

#[test]
fn a_constant_is_named_after_the_path_of_its_type() {
    let named = |uri: &str| constant_name(uri).unwrap();
    assert_eq!(
        named(&format!("{TRANSITIONAL}officeDocument")),
        "OFFICE_DOCUMENT"
    );
    assert_eq!(
        named(&format!("{TRANSITIONAL}customXmlProps")),
        "CUSTOM_XML_PROPERTIES"
    );
    assert_eq!(
        named(&format!("{TRANSITIONAL}extended-properties")),
        "EXTENDED_PROPERTIES"
    );
    assert_eq!(named(CORE), "CORE_PROPERTIES");
    assert_eq!(
        named("http://schemas.microsoft.com/office/2006/relationships/ui/userCustomization"),
        "UI_USER_CUSTOMIZATION"
    );
    constant_name("http://example.com/types/header").unwrap_err();
}

#[test]
fn the_table_of_document_kinds_reads_each_row() {
    let read = kinds(
        "[[kind]]\nfile = \"spreadsheet\"\nname = \"AddIn\"\ncontent_type = \"application/vnd.ms-excel.addin.macroEnabled.main+xml\"\n",
    )
    .unwrap();
    assert_eq!(
        read,
        [kind(
            "spreadsheet",
            "AddIn",
            "application/vnd.ms-excel.addin.macroEnabled.main+xml"
        )]
    );
    kinds("[[kind]]\nfile = \"spreadsheet\"\n").unwrap_err();
}

/// Part 1's definitions of two Comments parts, the Main Document part, two
/// parts of the `customXml` type, one stating it in the Transitional spelling,
/// and the Core File Properties part, which Part 2 defines.
fn definitions() -> Vec<PartDefinition> {
    let mut main = definition(
        "11.3.10",
        "Main Document Part",
        &[DOCUMENT],
        &format!("{STRICT}officeDocument"),
    );
    main.root_element = Some("document".to_owned());
    let core = PartDefinition {
        defined_in_part_2: true,
        section: "15.2.12.1".to_owned(),
        title: "Core File Properties Part".to_owned(),
        ..PartDefinition::default()
    };
    vec![
        definition(
            "11.3.2",
            "Comments Part",
            &[WORD_COMMENTS],
            &format!("{STRICT}comments"),
        ),
        main,
        definition(
            "12.3.3",
            "Comments Part",
            &[SHEET_COMMENTS],
            &format!("{STRICT}comments"),
        ),
        definition(
            "15.2.1",
            "Additional Characteristics Part",
            &["application/xml"],
            &format!("{TRANSITIONAL}customXml"),
        ),
        definition(
            "15.2.3",
            "Bibliography Part",
            &["application/xml"],
            &format!("{STRICT}customXml"),
        ),
        core,
    ]
}

fn restatements() -> Vec<TransitionalPart> {
    vec![
        restatement("9.2.2", "11.3.2", &format!("{TRANSITIONAL}comments")),
        restatement(
            "9.2.10",
            "11.3.10",
            &format!("{TRANSITIONAL}officeDocument"),
        ),
        restatement("10.2.3", "12.3.3", &format!("{TRANSITIONAL}comments")),
        restatement("13.2.1", "15.2.1", &format!("{TRANSITIONAL}customXml")),
        restatement("13.2.3", "15.2.3", &format!("{TRANSITIONAL}customXml")),
    ]
}

/// SDK 3.5.1's part data for the definitions, an extension part, and a
/// package, which is no part.
fn data() -> SdkData {
    let mut main = data_part(
        "MainDocumentPart",
        None,
        Some(&format!("{TRANSITIONAL}officeDocument")),
    );
    main.root_element = Some("document".to_owned());
    let mut package = data_part("WordprocessingDocument", None, None);
    package.base = Some("OpenXmlPackage".to_owned());
    SdkData {
        enumerations: BTreeMap::new(),
        namespaces: Vec::new(),
        parts: vec![
            data_part(
                "CoreFilePropertiesPart",
                Some("application/vnd.openxmlformats-package.core-properties+xml"),
                Some(CORE),
            ),
            data_part(
                "CustomXmlPart",
                None,
                Some(&format!("{TRANSITIONAL}customXml")),
            ),
            main,
            package,
            data_part(
                "WordprocessingCommentsExPart",
                Some(
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml",
                ),
                Some("http://schemas.microsoft.com/office/2011/relationships/commentsExtended"),
            ),
            data_part(
                "WordprocessingCommentsPart",
                Some(WORD_COMMENTS),
                Some(&format!("{TRANSITIONAL}comments")),
            ),
            data_part(
                "WorksheetCommentsPart",
                Some(SHEET_COMMENTS),
                Some(&format!("{TRANSITIONAL}comments")),
            ),
        ],
        schematrons: Vec::new(),
        typed_namespaces: BTreeMap::new(),
        types: BTreeMap::new(),
    }
}

fn word_kinds() -> [Kind; 2] {
    [
        kind("wordprocessing", "Document", DOCUMENT),
        kind("wordprocessing", "MacroEnabledDocument", MACRO_DOCUMENT),
    ]
}

#[test]
fn the_sources_join_into_one_list_of_part_types() {
    let joined = join(
        &definitions(),
        &restatements(),
        &data(),
        &word_kinds(),
        &BTreeMap::new(),
    )
    .unwrap();
    let modules: Vec<&str> = joined
        .parts
        .iter()
        .map(|part| part.module.as_str())
        .collect();
    assert_eq!(
        modules,
        [
            "wordprocessing_comments",
            "main_document",
            "spreadsheet_comments",
            "additional_characteristics",
            "bibliography",
            "core_file_properties",
            "wordprocessing_comments_ex",
        ]
    );
    assert_eq!(joined.parts[1].content_types, [DOCUMENT, MACRO_DOCUMENT]);
    assert_eq!(joined.parts[5].strict, CORE);
    assert_eq!(joined.parts[5].transitional, CORE);
    assert!(matches!(
        &joined.parts[6].definition,
        Definition::Extension(extension) if extension.name == "WordprocessingCommentsExPart"
    ));
}

#[test]
fn one_type_takes_one_constant_and_the_strict_spelling_of_strict() {
    let joined = join(
        &definitions(),
        &restatements(),
        &data(),
        &word_kinds(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(joined.parts[3].strict, format!("{STRICT}customXml"));
    assert_eq!(joined.notes.len(), 1, "{:?}", joined.notes);
    let constants: Vec<(&str, usize)> = joined
        .constants
        .iter()
        .map(|constant| (constant.name.as_str(), constant.modules.len()))
        .collect();
    assert_eq!(
        constants,
        [
            ("COMMENTS", 2),
            ("COMMENTS_EXTENDED", 1),
            ("CORE_PROPERTIES", 1),
            ("CUSTOM_XML", 2),
            ("OFFICE_DOCUMENT", 1),
        ]
    );
}

#[test]
fn a_main_part_content_type_without_a_kind_is_an_error() {
    join(
        &definitions(),
        &restatements(),
        &data(),
        &word_kinds()[1..],
        &BTreeMap::new(),
    )
    .unwrap_err();
}
