use super::ConformanceClass;

#[test]
fn a_class_is_enabled_when_its_feature_is_on() {
    assert_eq!(ConformanceClass::Strict.enabled(), cfg!(feature = "strict"));
    assert_eq!(
        ConformanceClass::Transitional.enabled(),
        cfg!(feature = "transitional")
    );
}

#[test]
fn the_office_document_relationship_names_the_class() {
    assert_eq!(
        ConformanceClass::from_office_document_relationship(
            "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument"
        ),
        Some(ConformanceClass::Strict)
    );
    assert_eq!(
        ConformanceClass::from_office_document_relationship(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
        ),
        Some(ConformanceClass::Transitional)
    );
    assert_eq!(
        ConformanceClass::from_office_document_relationship(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header"
        ),
        None
    );
}
