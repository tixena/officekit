use core::iter;

use super::{Validator, document_parts, reachable_parts, rule};
use crate::packaging::package::Package;
use crate::packaging::part_name::PartName;
use crate::validation::error::{ErrorCode, ValidationError};
use crate::xml::name::OfficeVersion;

const COMMENTS_EXTENDED: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const STRICT_OFFICE_DOCUMENT: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
const STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const UNKNOWN: &str = "http://example.com/relationships/unknown";

/// A Word document related by `office_document` whose main part relates to
/// one part by each of `related`.
fn word(office_document: &str, related: &[(&str, &str)]) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package.set_part_bytes(&main, MAIN, b"<a/>").unwrap();
    package.relate(None, &main, office_document).unwrap();
    for (name, relationship_type) in related {
        let part = PartName::new(name).unwrap();
        package
            .set_part_bytes(&part, "application/xml", b"<a/>")
            .unwrap();
        package
            .relate(Some(&main), &part, relationship_type)
            .unwrap();
    }
    package
}

fn reached(package: &Package, target: OfficeVersion) -> Vec<(String, &'static str)> {
    reachable_parts(package, target)
        .into_iter()
        .map(|(part, part_rule)| (part.as_str().to_owned(), part_rule.name))
        .collect()
}

fn errors(count: usize) -> Vec<ValidationError> {
    iter::repeat_with(|| ValidationError::new(ErrorCode::ChoiceWithoutRequires, None, None, &[]))
        .take(count)
        .collect()
}

#[test]
fn a_new_validator_stops_at_1000_errors() {
    let validator = Validator::new(OfficeVersion::Office2016);
    assert_eq!(Validator::DEFAULT_MAX_ERRORS, 1000);
    assert_eq!(validator.max_errors(), 1000);
    assert_eq!(validator.version(), OfficeVersion::Office2016);
}

#[test]
fn the_error_that_reaches_the_cap_is_kept() {
    let validator = Validator::new(OfficeVersion::Microsoft365).with_max_errors(3);
    assert_eq!(validator.limit(errors(2)).len(), 2);
    assert_eq!(validator.limit(errors(3)).len(), 3);
    assert_eq!(validator.limit(errors(4)).len(), 3);
}

#[test]
fn zero_means_no_limit() {
    let validator = Validator::new(OfficeVersion::Microsoft365).with_max_errors(0);
    assert_eq!(validator.limit(errors(2500)).len(), 2500);
}

#[test]
fn a_package_with_no_document_has_no_errors() {
    let validator = Validator::new(OfficeVersion::Microsoft365);
    assert_eq!(validator.validate(&Package::new()).len(), 0);
}

#[test]
fn each_relationship_type_names_one_part_type_in_a_document() {
    for document in [
        "PresentationDocument",
        "SpreadsheetDocument",
        "WordprocessingDocument",
    ] {
        let parts = document_parts(rule(document).unwrap());
        assert!(!parts.is_empty(), "{document}");
        for part_rule in &parts {
            let same = parts
                .iter()
                .filter(|other| other.relationship_type == part_rule.relationship_type)
                .count();
            assert_eq!(same, 1, "{document}: {}", part_rule.name);
        }
    }
}

#[test]
fn the_parts_reached_are_typed_by_relationship_and_kept_from_their_version_on() {
    let package = word(
        OFFICE_DOCUMENT,
        &[
            ("/word/styles.xml", STYLES),
            ("/word/commentsExtended.xml", COMMENTS_EXTENDED),
            ("/word/other.xml", UNKNOWN),
        ],
    );
    assert_eq!(
        reached(&package, OfficeVersion::Office2013),
        [
            ("/word/document.xml".to_owned(), "MainDocumentPart"),
            ("/word/styles.xml".to_owned(), "StyleDefinitionsPart"),
            (
                "/word/commentsExtended.xml".to_owned(),
                "WordprocessingCommentsExPart"
            ),
        ]
    );
    assert_eq!(
        reached(&package, OfficeVersion::Office2010),
        [
            ("/word/document.xml".to_owned(), "MainDocumentPart"),
            ("/word/styles.xml".to_owned(), "StyleDefinitionsPart"),
        ]
    );
}

#[test]
fn a_strict_relationship_type_reaches_the_same_parts() {
    let package = word(STRICT_OFFICE_DOCUMENT, &[]);
    assert_eq!(
        reached(&package, OfficeVersion::Microsoft365),
        [("/word/document.xml".to_owned(), "MainDocumentPart")]
    );
}
