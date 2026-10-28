use std::path::Path;

use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use os_shim::mock::MemorySystem;

use super::check;
use crate::validation::error::{ErrorCode, Errors, Finding, ValidationError};
use crate::validation::validator::Validator;

const DOCUMENT: &str = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body/></w:document>"#;
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
const SLIDE_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const SLIDE_XML: &str =
    r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>"#;
const STYLES: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const STYLES_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const STYLES_XML: &str =
    r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"/>"#;

/// One part the main part relates to: its name, content type, relationship
/// type, and XML.
type Related<'part> = (&'part str, &'part str, &'part str, &'part str);

fn codes(errors: &[ValidationError]) -> Vec<ErrorCode> {
    errors.iter().map(|error| error.code).collect()
}

fn names(error: &ValidationError) -> (Option<&str>, Option<&str>) {
    (
        error.part.as_ref().map(PartName::as_str),
        error.related_part.as_ref().map(PartName::as_str),
    )
}

/// The package pass's errors on `package`, saved to and read back from a
/// memory filesystem.
fn package_errors(package: &Package) -> Vec<ValidationError> {
    let mut errors = Errors::new(0);
    check(&reopened(package), OfficeVersion::Microsoft365, &mut errors);
    errors.into_vec()
}

fn reopened(package: &Package) -> Package {
    let system = MemorySystem::new().with_dir("/out").unwrap();
    package.save(&system, Path::new("/out/test.docx")).unwrap();
    Package::open(&system, Path::new("/out/test.docx")).unwrap()
}

/// `package` saved and read back with the relationship part of `source`
/// replaced by `entries`, so they parse as a file gives them.
fn with_relationship_entries(package: &Package, source: &PartName, entries: &str) -> Package {
    let mut written = package.duplicate().unwrap();
    written
        .set_part_bytes(
            &PartName::new(&source.relationships_part()).unwrap(),
            "application/vnd.openxmlformats-package.relationships+xml",
            format!("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{entries}</Relationships>").as_bytes(),
        )
        .unwrap();
    Package::from_bytes(written.to_bytes().unwrap()).unwrap()
}

/// A Word document whose main part, `/word/document.xml`, relates to each of
/// `related`.
fn word(related: &[Related<'_>]) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package
        .set_part_bytes(&main, MAIN, DOCUMENT.as_bytes())
        .unwrap();
    package.relate(None, &main, OFFICE_DOCUMENT).unwrap();
    for (name, content_type, relationship_type, xml) in related {
        let part = PartName::new(name).unwrap();
        package
            .set_part_bytes(&part, content_type, xml.as_bytes())
            .unwrap();
        package
            .relate(Some(&main), &part, relationship_type)
            .unwrap();
    }
    package
}

#[test]
fn a_docx_without_its_main_part_lacks_its_required_part() {
    let mut package = Package::new();
    // The extension's Default keeps the main part's content type after the
    // part is gone, so the relationship still names a word-processing document.
    let main = PartName::new("/word/document.main").unwrap();
    package
        .set_part_bytes(&main, MAIN, DOCUMENT.as_bytes())
        .unwrap();
    package.remove_part(&main).unwrap();
    package.relate(None, &main, OFFICE_DOCUMENT).unwrap();
    let errors = package_errors(&package);
    assert_eq!(codes(&errors), [ErrorCode::MissingRequiredPart]);
    assert_eq!(names(&errors[0]), (None, None));
    assert_eq!(
        errors[0].line(),
        "package MissingRequiredPart: the required part MainDocumentPart is missing"
    );
}

#[test]
fn a_second_styles_relationship_is_a_duplicate() {
    for second in ["/word/styles.xml", "/word/styles2.xml"] {
        let package = word(&[
            ("/word/styles.xml", STYLES, STYLES_TYPE, STYLES_XML),
            (second, STYLES, STYLES_TYPE, STYLES_XML),
        ]);
        let errors = package_errors(&package);
        assert_eq!(codes(&errors), [ErrorCode::DuplicatePart], "{second}");
        assert_eq!(names(&errors[0]), (Some("/word/document.xml"), None));
        assert_eq!(
            errors[0].message,
            "the part /word/document.xml can relate to only one StyleDefinitionsPart"
        );
    }
}

#[test]
fn the_main_document_cannot_relate_to_a_slide() {
    let package = word(&[("/ppt/slides/slide1.xml", SLIDE, SLIDE_TYPE, SLIDE_XML)]);
    let errors = package_errors(&package);
    assert_eq!(codes(&errors), [ErrorCode::PartNotAllowed]);
    assert_eq!(
        names(&errors[0]),
        (Some("/word/document.xml"), Some("/ppt/slides/slide1.xml"))
    );
    assert_eq!(
        errors[0].line(),
        "/word/document.xml: package PartNotAllowed: the part /word/document.xml cannot relate to the part /ppt/slides/slide1.xml"
    );
    // The empty slide fails the schema pass, which comes second, under the same cap.
    let validator = Validator::new(OfficeVersion::Microsoft365);
    let all = validator.validate(&reopened(&package));
    assert_eq!(all.first(), errors.first());
    assert!(all.len() > 1, "{all:?}");
    assert!(
        all.iter()
            .skip(1)
            .all(|error| error.code.kind() == Finding::Schema),
        "{all:?}"
    );
    assert_eq!(
        validator.with_max_errors(1).validate(&reopened(&package)),
        errors
    );
}

#[test]
fn a_standard_relationship_type_with_no_rule_is_an_unknown_part() {
    let package = word(&[
        (
            "/word/unknown.xml",
            "application/xml",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/unknown",
            "<a/>",
        ),
        (
            "/word/custom.xml",
            "application/xml",
            "http://example.com/relationships/custom",
            "<a/>",
        ),
    ]);
    let errors = package_errors(&package);
    assert_eq!(codes(&errors), [ErrorCode::UnknownPartOfStandardType]);
    assert_eq!(
        names(&errors[0]),
        (Some("/word/document.xml"), Some("/word/unknown.xml"))
    );
}

#[test]
fn a_part_whose_content_type_contradicts_its_part_type_gets_no_schema_error() {
    let package = word(&[(
        "/word/styles.xml",
        "application/xml",
        STYLES_TYPE,
        r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:bogus/></w:styles>"#,
    )]);
    let reopened = reopened(&package);
    let errors = Validator::new(OfficeVersion::Microsoft365).validate(&reopened);
    assert_eq!(codes(&errors), [ErrorCode::UnknownPartOfStandardType]);
    assert_eq!(
        names(&errors[0]),
        (Some("/word/document.xml"), Some("/word/styles.xml"))
    );
}

#[test]
fn a_clean_docx_has_no_error() {
    let package = word(&[("/word/styles.xml", STYLES, STYLES_TYPE, STYLES_XML)]);
    assert_eq!(package_errors(&package), []);
    assert_eq!(
        Validator::new(OfficeVersion::Microsoft365).validate(&reopened(&package)),
        []
    );
}

#[test]
fn a_relationship_target_that_names_no_part_does_not_hide_the_others() {
    let master = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
    let deck = PartName::new("/ppt/presentation.xml").unwrap();
    let mut package = Package::new();
    package
        .set_part_bytes(
            &deck,
            "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
            b"<p:presentation xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>",
        )
        .unwrap();
    package.relate(None, &deck, OFFICE_DOCUMENT).unwrap();
    package
        .set_part_bytes(
            &PartName::new("/ppt/slideMasters/slideMaster1.xml").unwrap(),
            "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml",
            b"<p:sldMaster xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>",
        )
        .unwrap();
    let entries = format!(
        r#"<Relationship Id="rId1" Type="{master}" Target="slideMasters/slideMaster1.xml"/><Relationship Id="rId2" Type="http://example.com/relationships/media" Target="media/"/>"#
    );
    let reread = with_relationship_entries(&package, &deck, &entries);
    let errors = package_errors(&reread);
    assert!(
        !codes(&errors).contains(&ErrorCode::MissingRequiredPart),
        "{errors:?}"
    );
}
