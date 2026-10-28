#[cfg(feature = "transitional")]
use crate::parts::document_settings;
#[cfg(feature = "transitional")]
use crate::parts::kinds::WordprocessingKind;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
#[cfg(feature = "transitional")]
use officekit_markup::wordprocessing::DocumentProtectionTypesMember;
#[cfg(feature = "transitional")]
use officekit_xml::conformance::ConformanceClass;
#[cfg(all(feature = "transitional", feature = "strict"))]
use officekit_xml::packaging::package::OpenOptions;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::part_name::PartName;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::relationship::{RelationshipType, Target};

use super::{hash, legacy_hash};
#[cfg(feature = "transitional")]
use crate::docx::Docx;

const ASPOSE_SALT: &str = "hIqOl99kIr/+N0hyovEq7Q==";

fn salt() -> [u8; 16] {
    STANDARD.decode(ASPOSE_SALT).unwrap().try_into().unwrap()
}

#[cfg(feature = "transitional")]
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
    let settings = settings_text(&docx);
    assert_eq!(settings.matches("documentProtection").count(), 1);
    assert!(settings.contains(r#"<w:documentProtection w:edit="comments" w:enforcement="1"/>"#));
    docx.protect(
        DocumentProtectionTypesMember::ReadOnly,
        "secret",
        &salt(),
        ConformanceClass::Transitional,
    )
    .unwrap();
    let protected = settings_text(&docx);
    assert_eq!(protected.matches("documentProtection").count(), 1);
    assert!(protected.contains(
        r#"<w:documentProtection w:edit="readOnly" w:enforcement="1" w:cryptProviderType="rsaFull" w:cryptAlgorithmClass="hash" w:cryptAlgorithmType="typeAny" w:cryptAlgorithmSid="4" w:cryptSpinCount="50000" w:hash="7tpTMDU2cZks7+gVqBJPt9DaBHg=" w:salt="hIqOl99kIr/+N0hyovEq7Q=="/>"#
    ));
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
    let settings = settings_text(&docx);
    let expected_hash = STANDARD.encode(hash("secret", &salt()));
    let expected = format!(
        r#"<w:documentProtection w:edit="readOnly" w:enforcement="1" w:algorithmName="SHA-1" w:hashValue="{expected_hash}" w:saltValue="{ASPOSE_SALT}" w:spinCount="50000"/>"#
    );
    assert!(settings.contains(&expected), "{settings}");
}

#[cfg(feature = "transitional")]
#[test]
fn an_empty_password_gives_the_restriction_alone_in_either_class() {
    for class in [ConformanceClass::Transitional, ConformanceClass::Strict] {
        let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
        docx.protect(DocumentProtectionTypesMember::Forms, "", &salt(), class)
            .unwrap();
        assert!(
            settings_text(&docx)
                .contains(r#"<w:documentProtection w:edit="forms" w:enforcement="1"/>"#)
        );
    }
}

#[cfg(feature = "transitional")]
#[test]
fn an_existing_entry_is_replaced_where_it_stands_among_the_other_settings() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let main = docx.main_document_part().clone();
    let name = PartName::new("/word/settings.xml").unwrap();
    let text = "<w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:zoom w:percent=\"100\"/><w:documentProtection w:edit=\"forms\" w:enforcement=\"1\"/><w:defaultTabStop w:val=\"720\"/></w:settings>";
    docx.package_mut()
        .set_part_bytes(&name, document_settings::CONTENT_TYPE, text.as_bytes())
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
    let settings = settings_text(&docx);
    let zoom = settings.find("<w:zoom").unwrap();
    let protection = settings.find("<w:documentProtection").unwrap();
    let tab_stop = settings.find("<w:defaultTabStop").unwrap();
    assert!(zoom < protection && protection < tab_stop, "{settings}");
    assert_eq!(settings.matches("documentProtection").count(), 1);
    assert!(settings.contains(r#"w:edit="comments""#));
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
