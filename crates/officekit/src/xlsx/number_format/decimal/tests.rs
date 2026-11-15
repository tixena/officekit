use super::Decimal;

/// `text` read, or a failed test.
fn read(text: &str) -> Decimal {
    Decimal::parse(text).unwrap()
}

#[test]
fn reads_the_lexical_forms_of_a_double() {
    for (text, fixed) in [
        ("1234.5", "1234.5"),
        ("-0.25", "0.25"),
        ("+7", "7"),
        (".5", "0.5"),
        ("5.", "5"),
        ("1.5E3", "1500"),
        ("12e-3", "0.012"),
        ("1E+2", "100"),
        (" 42 ", "42"),
        ("000.000", "0"),
    ] {
        assert_eq!(read(text).abs().fixed(), fixed, "{text}");
    }
    assert!(read("-0.25").is_negative());
    assert!(!read("-0").is_negative());
    assert!(read("-0").is_zero());
}

#[test]
fn no_number() {
    for text in [
        "INF", "-INF", "NaN", "", ".", "1e", "1e+", "--1", "1.2.3", "0x10", "1e400", "1,5",
    ] {
        assert_eq!(Decimal::parse(text), None, "{text}");
    }
    assert!(read("1e-400").is_zero());
}

#[test]
fn scales_and_rounds() {
    assert_eq!(read("12.2").scaled(6).fixed(), "12200000");
    assert_eq!(read("12200000").scaled(-6).fixed(), "12.2");
    assert_eq!(read("1.005").rounded(2).fixed(), "1.01");
    assert_eq!(read("-2.5").rounded(0), read("-3"));
    assert_eq!(read("0.0004").rounded(3).fixed(), "0");
    assert_eq!(read("0.0005").rounded(3).fixed(), "0.001");
    assert_eq!(read("0.00005").rounded(3).fixed(), "0");
    assert_eq!(read("999.96").rounded(1).fixed(), "1000");
    assert_eq!(read("1250").rounded(-2).fixed(), "1300");
}

#[test]
fn digits_and_exponent() {
    let number = read("1234.5678");
    assert_eq!(number.whole_digits(), "1234");
    assert_eq!(number.fraction_digits(6), "567800");
    assert_eq!(number.exponent(), Some(3));
    assert_eq!(read("0.012").exponent(), Some(-2));
    assert_eq!(read("0").exponent(), None);
    assert_eq!(read("0.5").whole_digits(), "");
}

#[test]
fn compares() {
    assert!(read("100") > read("99.999"));
    assert!(read("-100") < read("-99.999"));
    assert!(read("-1") < read("0"));
    assert_eq!(read("1E2"), read("100.0"));
    assert!(read("0.5") < read("0.51"));
}

#[test]
fn times_a_whole_number_rounded_a_half_upward() {
    assert_eq!(read("0.4207639").times_rounded(86_400), Some(36_354));
    assert_eq!(read("2.5").times_rounded(1), Some(3));
    assert_eq!(read("-2.5").times_rounded(1), Some(-2));
    assert_eq!(read("-2.51").times_rounded(1), Some(-3));
    assert_eq!(read("-2.00000001").times_rounded(86_400), Some(-172_800));
    assert_eq!(read("0").times_rounded(86_400), Some(0));
    assert_eq!(read("1e300").times_rounded(86_400), None);
}
