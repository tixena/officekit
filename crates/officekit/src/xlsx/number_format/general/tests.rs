use super::general;

#[test]
fn whole_and_decimal() {
    assert_eq!(general("1234.5678", 11).as_deref(), Some("1234.5678"));
    assert_eq!(general("0.1", 11).as_deref(), Some("0.1"));
    assert_eq!(general("-42", 11).as_deref(), Some("-42"));
    assert_eq!(general("0", 11).as_deref(), Some("0"));
    assert_eq!(general("-0", 11).as_deref(), Some("0"));
    assert_eq!(general("1.50", 11).as_deref(), Some("1.5"));
}

#[test]
fn eleven_characters() {
    assert_eq!(
        general("1234567.123456", 11).as_deref(),
        Some("1234567.123")
    );
    assert_eq!(
        general("0.123456789012", 11).as_deref(),
        Some("0.123456789")
    );
    assert_eq!(
        general("1234567.123456", 40).as_deref(),
        Some("1234567.123")
    );
    assert_eq!(
        general("-1234567.123456", 11).as_deref(),
        Some("-1234567.123")
    );
    assert_eq!(general("12345678901", 11).as_deref(), Some("12345678901"));
}

#[test]
fn a_narrow_cell() {
    assert_eq!(general("1234.5678", 6).as_deref(), Some("1234.6"));
    assert_eq!(general("1234.5678", 4).as_deref(), Some("1235"));
    assert_eq!(general("1234.5678", 5).as_deref(), Some("1235"));
    assert_eq!(general("0.5", 3).as_deref(), Some("0.5"));
    assert_eq!(general("0", 1).as_deref(), Some("0"));
    assert_eq!(general("0", 0), None);
}

#[test]
fn an_exponent() {
    assert_eq!(general("123456789012", 11).as_deref(), Some("1.23457E+11"));
    assert_eq!(
        general("0.00000000001234", 11).as_deref(),
        Some("1.234E-11")
    );
    assert_eq!(general("100000000000", 11).as_deref(), Some("1E+11"));
    assert_eq!(general("1e300", 11).as_deref(), Some("1E+300"));
    assert_eq!(general("-1.5E-200", 11).as_deref(), Some("-1.5E-200"));
    assert_eq!(general("999999999999", 11).as_deref(), Some("1E+12"));
    assert_eq!(general("123456", 5).as_deref(), Some("1E+05"));
    assert_eq!(general("9999.7", 4), None);
}

#[test]
fn below_one_with_more_than_three_zeros_after_the_point() {
    assert_eq!(
        general("0.0000123456789", 11).as_deref(),
        Some("1.23457E-05")
    );
    assert_eq!(general("0.00001234", 11).as_deref(), Some("0.00001234"));
    assert_eq!(
        general("0.000123456789", 11).as_deref(),
        Some("0.000123457")
    );
}

#[test]
fn no_room() {
    assert_eq!(general("123456", 4), None);
    assert_eq!(general("0.5", 2), None);
}
