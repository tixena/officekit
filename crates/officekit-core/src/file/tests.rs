use officekit_xml::conformance::ConformanceClass;

use super::class_enabled;

#[test]
fn a_class_is_enabled_when_its_feature_is_on() {
    assert_eq!(
        class_enabled(ConformanceClass::Strict),
        cfg!(feature = "strict")
    );
    assert_eq!(
        class_enabled(ConformanceClass::Transitional),
        cfg!(feature = "transitional")
    );
}
