use super::{failures, has_value};
use crate::validation::schema::types::{Bounds, Check, StringKind, ValueClass};
use crate::xml::name::OfficeVersion;

const NO_BOUNDS: Bounds = Bounds {
    max_exclusive: None,
    max_inclusive: None,
    min_exclusive: None,
    min_inclusive: None,
    non_negative: false,
    positive: false,
};

const ITEM: Check = Check::Number {
    bounds: NO_BOUNDS,
    class: ValueClass::Int32,
    list: false,
};

const fn number(class: ValueClass, bounds: Bounds) -> Check {
    Check::Number {
        bounds,
        class,
        list: false,
    }
}

const fn string(
    length: Option<usize>,
    max_length: Option<usize>,
    pattern: Option<&'static str>,
) -> Check {
    Check::String {
        class: None,
        kind: StringKind::Plain,
        length,
        max_length,
        min_length: None,
        pattern,
    }
}

const fn kind(kind: StringKind) -> Check {
    Check::String {
        class: None,
        kind,
        length: None,
        max_length: None,
        min_length: None,
        pattern: None,
    }
}

fn found(check: &Check, own: &ValueClass, value: &str) -> Vec<String> {
    failures(check, own, value, OfficeVersion::Microsoft365)
}

#[test]
fn a_whole_number_reads_with_white_space_around_and_a_sign() {
    let check = number(ValueClass::Int16, NO_BOUNDS);
    for value in ["5", " +5 ", "-32768", "\t0\n"] {
        assert_eq!(
            found(&check, &ValueClass::String, value).len(),
            0,
            "{value}"
        );
    }
    for value in ["100.0", "40000", "", "5a", "+-5"] {
        assert_eq!(
            found(&check, &ValueClass::String, value),
            ["not a whole number from -32768 to 32767"],
            "{value}"
        );
    }
}

#[test]
fn the_bounds_fold_into_the_range_a_whole_number_detail_names() {
    let at_least_zero = Bounds {
        min_inclusive: Some(0.0_f64),
        ..NO_BOUNDS
    };
    assert_eq!(
        found(
            &number(ValueClass::Int16, at_least_zero),
            &ValueClass::String,
            "100.0"
        ),
        ["not a whole number from 0 to 32767"]
    );
}

#[test]
fn each_failed_bound_is_one_failure() {
    let both = Bounds {
        min_inclusive: Some(0.0_f64),
        non_negative: true,
        ..NO_BOUNDS
    };
    let check = number(ValueClass::Int32, both);
    assert_eq!(found(&check, &ValueClass::String, "-5").len(), 2);
    assert_eq!(found(&check, &ValueClass::String, "5").len(), 0);
}

#[test]
fn a_real_number_reads_infinity_and_exponents() {
    let check = number(ValueClass::Double, NO_BOUNDS);
    for value in ["1.5", "-.5e-3", "INF", "-INF", "NaN", "5."] {
        assert_eq!(
            found(&check, &ValueClass::String, value).len(),
            0,
            "{value}"
        );
    }
    for value in ["1,5", "e5", "1e", "abc"] {
        assert_eq!(
            found(&check, &ValueClass::String, value),
            ["not a number"],
            "{value}"
        );
    }
}

#[test]
fn a_string_check_with_no_class_counts_bytes_for_a_hexadecimal_attribute() {
    let four = string(Some(4), None, None);
    assert_eq!(found(&four, &ValueClass::HexBinary, "00AB12CD").len(), 0);
    assert_eq!(
        found(&four, &ValueClass::String, "00AB12CD"),
        ["8 characters long, not 4"]
    );
    assert_eq!(
        found(&four, &ValueClass::HexBinary, "0G"),
        ["not an even number of hexadecimal digits"]
    );
}

#[test]
fn a_pattern_failure_and_a_length_failure_are_two_failures() {
    let check = string(None, Some(2), Some("[0-9]+"));
    assert_eq!(
        found(&check, &ValueClass::String, "abc"),
        [
            "does not match the pattern [0-9]+",
            "3 characters long, more than 2"
        ]
    );
    assert_eq!(found(&check, &ValueClass::String, "12").len(), 0);
}

#[test]
fn a_lexical_kind_is_checked_in_place_of_the_pattern_when_it_fails() {
    let token = kind(StringKind::Token);
    assert_eq!(found(&token, &ValueClass::String, "a  b"), ["not a token"]);
    assert_eq!(found(&token, &ValueClass::String, "").len(), 0);
    let name = kind(StringKind::NcName);
    assert_eq!(found(&name, &ValueClass::String, "1abc"), ["not an NCName"]);
    assert_eq!(found(&name, &ValueClass::String, ""), ["empty"]);
    assert_eq!(found(&name, &ValueClass::String, "_a.b-c").len(), 0);
    let qualified = kind(StringKind::QName);
    assert_eq!(found(&qualified, &ValueClass::String, "w:val").len(), 0);
    assert_eq!(found(&qualified, &ValueClass::String, "w:").len(), 1);
}

#[test]
fn an_enumeration_member_counts_only_at_the_targets_it_is_one_at() {
    let check = Check::Enum {
        class: ValueClass::Enum("a15:ST_TargetScreenSz"),
    };
    assert_eq!(
        failures(
            &check,
            &ValueClass::String,
            "800x600",
            OfficeVersion::Office2013
        )
        .len(),
        0
    );
    assert_eq!(
        failures(
            &check,
            &ValueClass::String,
            "800x600",
            OfficeVersion::Office2010
        )
        .len(),
        1
    );
    let justification = Check::Enum {
        class: ValueClass::Enum("w:ST_Jc"),
    };
    assert_eq!(found(&justification, &ValueClass::String, "start").len(), 0);
    assert_eq!(found(&justification, &ValueClass::String, "Start").len(), 1);
}

#[test]
fn a_list_fails_when_it_is_empty_or_an_item_fails() {
    let check = Check::List { item: &ITEM };
    let own = ValueClass::List(&ValueClass::Int32);
    assert_eq!(found(&check, &own, "1 2\t3").len(), 0);
    assert_eq!(found(&check, &own, "1 x 3").len(), 1);
    assert_eq!(found(&check, &own, "  "), ["an empty list"]);
}

#[test]
fn the_boolean_classes_take_their_own_spellings() {
    let on_off = Check::Enum {
        class: ValueClass::OnOff,
    };
    for value in ["true", "false", "on", "off", "1", "0"] {
        assert_eq!(
            found(&on_off, &ValueClass::OnOff, value).len(),
            0,
            "{value}"
        );
    }
    assert_eq!(found(&on_off, &ValueClass::OnOff, "yes").len(), 1);
    let blank = Check::Enum {
        class: ValueClass::TrueFalseBlank,
    };
    assert_eq!(found(&blank, &ValueClass::TrueFalseBlank, "").len(), 0);
    assert_eq!(found(&blank, &ValueClass::TrueFalseBlank, "1").len(), 1);
}

#[test]
fn a_date_time_reads_a_date_and_time_a_date_or_a_time() {
    let check = number(ValueClass::DateTime, NO_BOUNDS);
    for value in [
        "2020-01-01T00:00:00Z",
        "2020-01-01",
        "13:00:00",
        "2020-02-29T10:00:00.5+05:00",
    ] {
        assert_eq!(
            found(&check, &ValueClass::String, value).len(),
            0,
            "{value}"
        );
    }
    for value in ["2021-02-29", "nope", "2020-13-01T00:00:00"] {
        assert_eq!(
            found(&check, &ValueClass::String, value),
            ["not a date and time"],
            "{value}"
        );
    }
}

#[test]
fn base64_reads_skipping_white_space_and_its_length_counts_decoded_bytes() {
    let three = Check::String {
        class: Some(ValueClass::Base64Binary),
        kind: StringKind::Plain,
        length: Some(3),
        max_length: None,
        min_length: None,
        pattern: None,
    };
    assert_eq!(found(&three, &ValueClass::String, "QUJD").len(), 0);
    assert_eq!(found(&three, &ValueClass::String, "QU JD").len(), 0);
    assert_eq!(
        found(&three, &ValueClass::String, "QUI="),
        ["2 bytes long, not 3"]
    );
    assert_eq!(
        found(&three, &ValueClass::String, "QU*D"),
        ["not base64 data"]
    );
}

#[test]
fn a_value_is_there_when_it_reads_as_its_class() {
    let target = OfficeVersion::Microsoft365;
    assert!(has_value(&ValueClass::String, "", target));
    assert!(has_value(&ValueClass::Int32, "7", target));
    assert!(!has_value(&ValueClass::Int32, "x", target));
    assert!(!has_value(
        &ValueClass::List(&ValueClass::String),
        " ",
        target
    ));
}
