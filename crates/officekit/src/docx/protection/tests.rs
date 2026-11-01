#[cfg(feature = "transitional")]
use crate::parts::document_settings;
#[cfg(feature = "transitional")]
use crate::parts::kinds::WordprocessingKind;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
#[cfg(feature = "transitional")]
use officekit_markup::common_simple_types::{MeasurementInTwentiethsOfAPoint, OnOffValue};
#[cfg(feature = "transitional")]
use officekit_markup::wordprocessing::{
    DistanceBetweenAutomaticTabStops, DocumentEditingRestrictions, DocumentProtectionTypes,
    DocumentProtectionTypesMember, DocumentSettings, MagnificationSetting, PercentageMeasurement,
};
#[cfg(feature = "transitional")]
use officekit_xml::conformance::ConformanceClass;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::package::OpenOptions;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::part_name::PartName;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::relationship::{RelationshipType, Target};
#[cfg(feature = "transitional")]
use officekit_xml::write::WriteOptions;

use super::{hash, legacy_hash};
#[cfg(feature = "transitional")]
use crate::docx::Docx;

const ASPOSE_SALT: &str = "hIqOl99kIr/+N0hyovEq7Q==";

fn salt() -> [u8; 16] {
    STANDARD.decode(ASPOSE_SALT).unwrap().try_into().unwrap()
}

/// The settings part of `docx`, read back through its part module.
#[cfg(feature = "transitional")]
fn settings(docx: &Docx) -> DocumentSettings {
    let related = docx
        .package()
        .related_by_type(Some(docx.main_document_part()), &RelationshipType::SETTINGS)
        .unwrap();
    assert_eq!(related.len(), 1);
    let Target::Part(name) = &related[0].target else {
        return DocumentSettings::default();
    };
    document_settings::load(docx.package(), name, OpenOptions::default()).unwrap()
}

/// The attributes the document protection of `docx` sets, by name, in the order the schema
/// declares them; the settings keep nothing else of it as unknown content.
#[cfg(feature = "transitional")]
fn protection(docx: &Docx) -> Vec<(&'static str, String)> {
    let read = settings(docx);
    assert_eq!(read.unknown.children, Vec::new());
    let entry = read.document_editing_restrictions.unwrap_or_default();
    [
        ("edit", entry.edit.map(|value| value.0)),
        ("enforcement", entry.enforcement.map(|value| value.0)),
        (
            "cryptProviderType",
            entry.crypt_provider_type.map(|value| value.0),
        ),
        (
            "cryptAlgorithmClass",
            entry.crypt_algorithm_class.map(|value| value.0),
        ),
        (
            "cryptAlgorithmType",
            entry.crypt_algorithm_type.map(|value| value.0),
        ),
        (
            "cryptAlgorithmSid",
            entry.crypt_algorithm_sid.map(|value| value.0),
        ),
        (
            "cryptSpinCount",
            entry.crypt_spin_count.map(|value| value.0),
        ),
        ("hash", entry.hash),
        ("salt", entry.salt),
        ("algorithmName", entry.algorithm_name),
        ("hashValue", entry.hash_value),
        ("saltValue", entry.salt_value),
        ("spinCount", entry.spin_count.map(|value| value.0)),
    ]
    .into_iter()
    .filter_map(|(attribute, value)| Some((attribute, value?)))
    .collect()
}

/// `pairs` as owned attribute values.
#[cfg(feature = "transitional")]
fn set(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    pairs
        .iter()
        .map(|(attribute, value)| (*attribute, (*value).to_owned()))
        .collect()
}

#[cfg(all(feature = "transitional", feature = "strict"))]
fn settings_text(docx: &Docx) -> String {
    let related = docx
        .package()
        .related_by_type(Some(docx.main_document_part()), &RelationshipType::SETTINGS)
        .unwrap();
    assert_eq!(related.len(), 1);
    let Target::Part(name) = &related[0].target else {
        return String::new();
    };
    String::from_utf8(docx.package().part_bytes(name).unwrap()).unwrap()
}

#[test]
fn the_legacy_hash_is_the_one_aspose_writes() {
    for (password, expected) in [
        ("secret", "7tpTMDU2cZks7+gVqBJPt9DaBHg="),
        ("other", "MU/fbtz6kxru4aiWrXR0y4c7s8k="),
        (
            "a much longer password than fifteen",
            "8lRrsNwxqms7IHMuJp489aMn+6k=",
        ),
    ] {
        assert_eq!(
            STANDARD.encode(legacy_hash(password, &salt())),
            expected,
            "{password}"
        );
    }
}

#[test]
fn the_part_1_hash_starts_from_the_password_itself() {
    assert_ne!(hash("secret", &salt()), legacy_hash("secret", &salt()));
}

#[cfg(feature = "transitional")]
#[test]
fn protecting_twice_leaves_one_entry_and_aspose_s_for_secret() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    docx.protect(
        DocumentProtectionTypesMember::ReadOnly,
        "secret",
        &salt(),
        ConformanceClass::Transitional,
    )
    .unwrap();
    docx.protect(
        DocumentProtectionTypesMember::Comments,
        "",
        &salt(),
        ConformanceClass::Transitional,
    )
    .unwrap();
    assert_eq!(
        protection(&docx),
        set(&[("edit", "comments"), ("enforcement", "1")])
    );
    docx.protect(
        DocumentProtectionTypesMember::ReadOnly,
        "secret",
        &salt(),
        ConformanceClass::Transitional,
    )
    .unwrap();
    assert_eq!(
        protection(&docx),
        set(&[
            ("edit", "readOnly"),
            ("enforcement", "1"),
            ("cryptProviderType", "rsaFull"),
            ("cryptAlgorithmClass", "hash"),
            ("cryptAlgorithmType", "typeAny"),
            ("cryptAlgorithmSid", "4"),
            ("cryptSpinCount", "50000"),
            ("hash", "7tpTMDU2cZks7+gVqBJPt9DaBHg="),
            ("salt", ASPOSE_SALT),
        ])
    );
}

#[cfg(feature = "transitional")]
#[test]
fn the_strict_entry_carries_part_1_attributes() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    docx.protect(
        DocumentProtectionTypesMember::ReadOnly,
        "secret",
        &salt(),
        ConformanceClass::Strict,
    )
    .unwrap();
    let expected_hash = STANDARD.encode(hash("secret", &salt()));
    assert_eq!(
        protection(&docx),
        set(&[
            ("edit", "readOnly"),
            ("enforcement", "1"),
            ("algorithmName", "SHA-1"),
            ("hashValue", &expected_hash),
            ("saltValue", ASPOSE_SALT),
            ("spinCount", "50000"),
        ])
    );
}

#[cfg(feature = "transitional")]
#[test]
fn an_empty_password_gives_the_restriction_alone_in_either_class() {
    for class in [ConformanceClass::Transitional, ConformanceClass::Strict] {
        let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
        docx.protect(DocumentProtectionTypesMember::Forms, "", &salt(), class)
            .unwrap();
        assert_eq!(
            protection(&docx),
            set(&[("edit", "forms"), ("enforcement", "1")]),
            "{class:?}"
        );
    }
}

#[cfg(feature = "transitional")]
#[test]
fn an_existing_entry_is_replaced_where_it_stands_among_the_other_settings() {
    let mut zoom = MagnificationSetting::default();
    zoom.percent = Some(PercentageMeasurement::from("100".to_owned()));
    let mut entry = DocumentEditingRestrictions::default();
    entry.edit = Some(DocumentProtectionTypes::from(
        DocumentProtectionTypesMember::Forms,
    ));
    entry.enforcement = Some(OnOffValue::from("1".to_owned()));
    let mut tab_stop = DistanceBetweenAutomaticTabStops::default();
    tab_stop.val = Some(MeasurementInTwentiethsOfAPoint::from("720".to_owned()));
    let mut held = DocumentSettings::default();
    held.distance_between_automatic_tab_stops = Some(Box::new(tab_stop));
    held.document_editing_restrictions = Some(Box::new(entry));
    held.magnification_setting = Some(Box::new(zoom));
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let main = docx.main_document_part().clone();
    let name = PartName::new("/word/settings.xml").unwrap();
    document_settings::store_transitional(
        docx.package_mut(),
        &name,
        &held,
        WriteOptions::default(),
    )
    .unwrap();
    docx.package_mut()
        .relate(Some(&main), &name, RelationshipType::SETTINGS.transitional)
        .unwrap();
    docx.protect(
        DocumentProtectionTypesMember::Comments,
        "",
        &salt(),
        ConformanceClass::Transitional,
    )
    .unwrap();
    let text = String::from_utf8(docx.package().part_bytes(&name).unwrap()).unwrap();
    let zoom_at = text.find("<w:zoom").unwrap();
    let protection_at = text.find("<w:documentProtection").unwrap();
    let tab_stop_at = text.find("<w:defaultTabStop").unwrap();
    assert!(
        zoom_at < protection_at && protection_at < tab_stop_at,
        "{text}"
    );
    assert_eq!(
        protection(&docx),
        set(&[("edit", "comments"), ("enforcement", "1")])
    );
}

#[cfg(all(feature = "transitional", feature = "strict"))]
#[test]
fn the_strict_entry_survives_a_strict_save() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    docx.protect(
        DocumentProtectionTypesMember::ReadOnly,
        "secret",
        &salt(),
        ConformanceClass::Strict,
    )
    .unwrap();
    let saved = Docx::from_bytes(docx.to_bytes_strict().unwrap(), OpenOptions::default()).unwrap();
    assert_eq!(saved.conformance_class(), ConformanceClass::Strict);
    let settings = settings_text(&saved);
    assert!(
        settings.contains(r#"w:algorithmName="SHA-1""#),
        "{settings}"
    );
    assert!(settings.contains(r#"w:spinCount="50000""#), "{settings}");
    assert_eq!(settings.matches("documentProtection").count(), 1);
}

#[cfg(feature = "transitional")]
#[test]
fn protecting_a_new_document_creates_the_settings_part_with_its_content_type() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    docx.protect(
        DocumentProtectionTypesMember::ReadOnly,
        "",
        &salt(),
        ConformanceClass::Transitional,
    )
    .unwrap();
    let name = PartName::new("/word/settings.xml").unwrap();
    assert_eq!(
        docx.package().content_type(&name),
        Some(document_settings::CONTENT_TYPE)
    );
    assert!(
        docx.package()
            .content_type_overrides()
            .contains(&(name, document_settings::CONTENT_TYPE.to_owned()))
    );
}
