use core::iter;

use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;

use super::{Validator, document_parts, reachable_parts, rule};
use crate::validation::error::{ErrorCode, ValidationError};

const CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const CHART_CONTENT: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const COMMENTS_EXTENDED: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";
const COMMENTS_EXTENDED_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml";
const NA_AS_BLANK_CHART: &[u8] = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:layout/></c:plotArea><c:extLst><c:ext xmlns:c16r3="http://schemas.microsoft.com/office/drawing/2017/03/chart" uri="{56B9EC1D-385E-4148-901F-78D8002777C0}"><c16r3:dataDisplayOptions16><c16r3:dispNaAsBlank val="1"/></c16r3:dataDisplayOptions16></c:ext></c:extLst></c:chart></c:chartSpace>"#;
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const STRICT_OFFICE_DOCUMENT: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
const STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const STYLES_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const UNKNOWN: &str = "http://example.com/relationships/unknown";

/// A Word document related by `office_document` whose main part relates to
/// one part by each of `related`: its name, relationship type, and content
/// type.
fn word(office_document: &str, related: &[(&str, &str, &str)]) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package.set_part_bytes(&main, MAIN, b"<a/>").unwrap();
    package.relate(None, &main, office_document).unwrap();
    for (name, relationship_type, content_type) in related {
        let part = PartName::new(name).unwrap();
        package
            .set_part_bytes(&part, content_type, b"<a/>")
            .unwrap();
        package
            .relate(Some(&main), &part, relationship_type)
            .unwrap();
    }
    package
}

fn chart_document(chart: &[u8]) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package
        .set_part_bytes(
            &main,
            MAIN,
            br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body/></w:document>"#,
        )
        .unwrap();
    package.relate(None, &main, OFFICE_DOCUMENT).unwrap();
    let part = PartName::new("/word/charts/chart1.xml").unwrap();
    package.set_part_bytes(&part, CHART_CONTENT, chart).unwrap();
    package.relate(Some(&main), &part, CHART).unwrap();
    package
}

fn errors_in_the_extension_list(package: &Package, version: OfficeVersion) -> Vec<String> {
    Validator::new(version)
        .validate(package)
        .iter()
        .filter(|error| error.path_text().contains("c:extLst"))
        .map(ValidationError::line)
        .collect()
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
            ("/word/styles.xml", STYLES, STYLES_CONTENT),
            (
                "/word/commentsExtended.xml",
                COMMENTS_EXTENDED,
                COMMENTS_EXTENDED_CONTENT,
            ),
            ("/word/other.xml", UNKNOWN, "application/xml"),
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
fn a_part_whose_content_type_contradicts_its_part_type_is_not_reached() {
    let package = word(
        OFFICE_DOCUMENT,
        &[("/word/styles.xml", STYLES, "application/xml")],
    );
    assert_eq!(
        reached(&package, OfficeVersion::Microsoft365),
        [("/word/document.xml".to_owned(), "MainDocumentPart")]
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

#[test]
fn a_strict_extended_properties_type_reaches_the_extended_properties_part() {
    let mut package = word(OFFICE_DOCUMENT, &[]);
    let app = PartName::new("/docProps/app.xml").unwrap();
    package
        .set_part_bytes(
            &app,
            "application/vnd.openxmlformats-officedocument.extended-properties+xml",
            b"<a/>",
        )
        .unwrap();
    package
        .relate(None, &app, RelationshipType::EXTENDED_PROPERTIES.strict)
        .unwrap();
    assert!(
        reached(&package, OfficeVersion::Microsoft365)
            .contains(&("/docProps/app.xml".to_owned(), "ExtendedFilePropertiesPart"))
    );
}

#[test]
fn a_target_that_names_no_part_does_not_hide_the_other_parts_of_its_source() {
    let main = PartName::new("/word/document.xml").unwrap();
    let mut package = word(
        OFFICE_DOCUMENT,
        &[("/word/styles.xml", STYLES, STYLES_CONTENT)],
    );
    package
        .set_part_bytes(
            &PartName::new(&main.relationships_part()).unwrap(),
            "application/vnd.openxmlformats-package.relationships+xml",
            format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{UNKNOWN}" Target="media/"/><Relationship Id="rId2" Type="{STYLES}" Target="styles.xml"/></Relationships>"#
            )
            .as_bytes(),
        )
        .unwrap();
    let reread = Package::from_bytes(package.to_bytes().unwrap()).unwrap();
    assert_eq!(
        reached(&reread, OfficeVersion::Office2013),
        [
            ("/word/document.xml".to_owned(), "MainDocumentPart"),
            ("/word/styles.xml".to_owned(), "StyleDefinitionsPart"),
        ]
    );
}

#[test]
fn a_chart_extension_list_accepts_the_extension_word_writes() {
    let package = chart_document(NA_AS_BLANK_CHART);
    for version in [
        OfficeVersion::Office2010,
        OfficeVersion::Office2013,
        OfficeVersion::Office2016,
        OfficeVersion::Office2019,
        OfficeVersion::Office2021,
        OfficeVersion::Microsoft365,
    ] {
        assert_eq!(
            errors_in_the_extension_list(&package, version),
            Vec::<String>::new(),
            "{version:?}"
        );
    }
}

#[test]
fn a_chart_extension_without_its_content_element_is_refused() {
    let misnested = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:layout/></c:plotArea><c:extLst><c:ext xmlns:c16r3="http://schemas.microsoft.com/office/drawing/2017/03/chart" uri="{56B9EC1D-385E-4148-901F-78D8002777C0}"><c16r3:dispNaAsBlank val="1"/></c:ext></c:extLst></c:chart></c:chartSpace>"#;
    let errors =
        errors_in_the_extension_list(&chart_document(misnested), OfficeVersion::Microsoft365);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("InvalidChild"), "{errors:?}");
}
