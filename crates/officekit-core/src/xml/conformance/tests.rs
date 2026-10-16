use super::ConformanceClass;

#[test]
fn a_class_is_enabled_when_its_feature_is_on() {
    assert_eq!(ConformanceClass::Strict.enabled(), cfg!(feature = "strict"));
    assert_eq!(
        ConformanceClass::Transitional.enabled(),
        cfg!(feature = "transitional")
    );
}
