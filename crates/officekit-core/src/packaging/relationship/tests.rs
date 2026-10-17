use core::slice;

use officekit_xml::conformance::ConformanceClass;
use officekit_xml::parse::parse as parse_xml;
use officekit_xml::write::WriteOptions;

use super::{
    Relationship, RelationshipType, Target, TargetMode, class_of_office_document, parse, relative,
    resolve, to_document,
};
use crate::packaging::part_name::PartName;

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

#[test]
fn a_target_resolves_against_its_sources_folder_with_dot_dot() {
    assert_eq!(
        resolve("/word/charts", "../embeddings/sheet.xlsx").unwrap(),
        name("/word/embeddings/sheet.xlsx")
    );
    assert_eq!(
        resolve("/word", "./media/image1.png").unwrap(),
        name("/word/media/image1.png")
    );
    assert_eq!(
        resolve("", "word/document.xml").unwrap(),
        name("/word/document.xml")
    );
    assert_eq!(
        resolve("/word", "/customXml/item1.xml").unwrap(),
        name("/customXml/item1.xml")
    );
    assert_eq!(resolve("/word", "../../../a.xml").unwrap(), name("/a.xml"));
    resolve("/word", "media/").unwrap_err();
}

#[test]
fn a_target_is_written_relative_to_its_sources_folder() {
    assert_eq!(
        relative("/word/charts", &name("/word/embeddings/sheet.xlsx")),
        "../embeddings/sheet.xlsx"
    );
    assert_eq!(
        relative("", &name("/word/document.xml")),
        "word/document.xml"
    );
    assert_eq!(relative("/word", &name("/word/header1.xml")), "header1.xml");
    assert_eq!(
        relative("/word", &name("/customXml/item1.xml")),
        "../customXml/item1.xml"
    );
    assert_eq!(
        relative("/customXml", &name("/customXml/itemProps1.xml")),
        "itemProps1.xml"
    );
    assert_eq!(relative("/a/b", &name("/a/b/c/d.xml")), "c/d.xml");
}

#[test]
fn a_relative_target_resolves_back_to_its_part() {
    for (directory, target) in [
        ("/word/charts", "/word/embeddings/sheet.xlsx"),
        ("", "/word/document.xml"),
        ("/ppt/slides", "/ppt/slideLayouts/slideLayout1.xml"),
        ("/word", "/word/media/image1.png"),
    ] {
        let part = name(target);
        assert_eq!(
            resolve(directory, &relative(directory, &part)).unwrap(),
            part
        );
    }
}

#[test]
fn an_internal_relationship_resolves_and_an_external_one_does_not() {
    let relationships = parse(&parse_xml(b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"t\" Target=\"media/image1.png\"/><Relationship Id=\"rId2\" Type=\"h\" Target=\"https://example.org/\" TargetMode=\"External\"/></Relationships>").unwrap()).unwrap();
    assert_eq!(
        relationships[0].resolved("/word").unwrap(),
        Target::Part(name("/word/media/image1.png"))
    );
    assert_eq!(relationships[1].target_mode, TargetMode::External);
    assert_eq!(
        relationships[1].resolved("/word").unwrap(),
        Target::External("https://example.org/".to_owned())
    );
}

#[test]
fn relationships_write_back_as_read() {
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"https://example.org/?a=1&amp;b=2\" TargetMode=\"External\"/></Relationships>";
    let relationships = parse(&parse_xml(xml.as_bytes()).unwrap()).unwrap();
    assert_eq!(relationships[1].target, "https://example.org/?a=1&b=2");
    let written = to_document(&relationships).to_xml(WriteOptions::default());
    assert_eq!(String::from_utf8(written).unwrap(), xml);
}

#[test]
fn a_relationship_without_a_required_attribute_is_refused() {
    let missing = parse(
        &parse_xml(b"<Relationships><Relationship Id=\"rId1\" Type=\"t\"/></Relationships>")
            .unwrap(),
    );
    assert_eq!(missing, Err("a relationship without Target".to_owned()));
    let root = parse(&parse_xml(b"<Types/>").unwrap());
    assert_eq!(
        root,
        Err("the root is <Types>, not <Relationships>".to_owned())
    );
}

#[test]
fn a_relationship_type_matches_either_spelling_and_gives_each_class_its_own() {
    let header = RelationshipType::HEADER;
    assert_eq!(
        header.transitional,
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header"
    );
    assert_eq!(
        header.in_class(ConformanceClass::Strict),
        "http://purl.oclc.org/ooxml/officeDocument/relationships/header"
    );
    assert_eq!(
        header.in_class(ConformanceClass::Transitional),
        header.transitional
    );
    assert!(header.matches(header.strict));
    assert!(header.matches(header.transitional));
    assert!(!header.matches(RelationshipType::IMAGE.transitional));
    assert_eq!(
        RelationshipType::CORE_PROPERTIES.strict,
        RelationshipType::CORE_PROPERTIES.transitional
    );
}

#[test]
fn every_written_field_is_kept() {
    let relationship = Relationship {
        id: "rId7".to_owned(),
        target: "../media/image1.png".to_owned(),
        target_mode: TargetMode::Internal,
        type_uri: RelationshipType::IMAGE.strict.to_owned(),
    };
    let document = to_document(slice::from_ref(&relationship));
    assert_eq!(parse(&document).unwrap(), [relationship]);
}

#[test]
fn the_office_document_relationship_names_the_class() {
    assert_eq!(
        class_of_office_document(
            "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument"
        ),
        Some(ConformanceClass::Strict)
    );
    assert_eq!(
        class_of_office_document(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
        ),
        Some(ConformanceClass::Transitional)
    );
    assert_eq!(
        class_of_office_document(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header"
        ),
        None
    );
}
