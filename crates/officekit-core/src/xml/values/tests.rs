use super::parse;
use super::{DateTime, Measure, MeasureUnit};

#[test]
fn booleans_accept_the_four_xsd_spellings_only() {
    assert_eq!(parse::boolean("true"), Some(true));
    assert_eq!(parse::boolean("1"), Some(true));
    assert_eq!(parse::boolean("false"), Some(false));
    assert_eq!(parse::boolean("0"), Some(false));
    for text in ["on", "off", "yes", "True", "", " 1"] {
        assert_eq!(parse::boolean(text), None, "{text:?}");
    }
}

#[test]
fn integers_parse_signed_whole_numbers() {
    assert_eq!(parse::integer("5000"), Some(5000));
    assert_eq!(parse::integer("-7"), Some(-7));
    assert_eq!(parse::integer("2.5"), None);
    assert_eq!(parse::integer(""), None);
}

#[test]
fn decimals_with_a_suffix_need_the_suffix_and_digits_on_both_sides_of_the_point() {
    assert_eq!(parse::decimal_with_suffix("50%", "%"), Some(50.0_f64));
    assert_eq!(parse::decimal_with_suffix("-12.5%", "%"), Some(-12.5_f64));
    assert_eq!(parse::decimal_with_suffix("-12.5", "%"), None);
    assert_eq!(parse::decimal_with_suffix("5.%", "%"), None);
    assert_eq!(parse::decimal_with_suffix(".5%", "%"), None);
    assert_eq!(parse::decimal_with_suffix("%", "%"), None);
    assert_eq!(parse::decimal_with_suffix("1e3%", "%"), None);
}

#[test]
fn measures_read_the_six_unit_spellings() {
    let cases = [
        ("2.5cm", MeasureUnit::Centimeters, 2.5_f64),
        ("1in", MeasureUnit::Inches, 1.0_f64),
        ("-3mm", MeasureUnit::Millimeters, -3.0_f64),
        ("10pc", MeasureUnit::Picas, 10.0_f64),
        ("10pi", MeasureUnit::Picas, 10.0_f64),
        ("12pt", MeasureUnit::Points, 12.0_f64),
    ];
    for (text, unit, value) in cases {
        assert_eq!(
            parse::measure(text),
            Some(Measure { unit, value }),
            "{text}"
        );
    }
    for text in ["2.5", "2.5px", "cm", "2.cm", "5\u{e9}", ""] {
        assert_eq!(parse::measure(text), None, "{text:?}");
    }
}

#[test]
fn lists_split_on_xml_whitespace_and_drop_empty_items() {
    assert_eq!(parse::list("").count(), 0);
    assert_eq!(parse::list(" a \r\n b ").collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(
        parse::list("A1:B2  D4\tF6:F9").collect::<Vec<_>>(),
        ["A1:B2", "D4", "F6:F9"]
    );
}

#[test]
fn dates_read_zone_fraction_and_negative_years() {
    assert_eq!(
        parse::date_time("2026-10-15T09:30:05Z"),
        Some(DateTime {
            day: 15,
            hour: 9,
            minute: 30,
            month: 10,
            nanosecond: 0,
            offset_minutes: Some(0),
            second: 5,
            year: 2026,
        })
    );
    let offset = parse::date_time("2024-02-29T23:59:59.25-04:30").unwrap();
    assert_eq!(
        (offset.nanosecond, offset.offset_minutes),
        (250_000_000, Some(-270))
    );
    let east = parse::date_time("2026-10-15T09:30:00+05:30").unwrap();
    assert_eq!(east.offset_minutes, Some(330));
    let zoneless = parse::date_time("2026-10-15T09:30:00").unwrap();
    assert_eq!(zoneless.offset_minutes, None);
    let nanos = parse::date_time("2026-10-15T09:30:00.123456789Z").unwrap();
    assert_eq!(nanos.nanosecond, 123_456_789);
    let ancient = parse::date_time("-0044-03-15T12:00:00Z").unwrap();
    assert_eq!(ancient.year, -44_i32);
}

#[test]
fn dates_refuse_what_xsd_date_time_refuses() {
    for text in [
        "2025-02-29T00:00:00",
        "2026-10-15",
        "2026-10-15T24:00:00",
        "2026-13-01T00:00:00",
        "2026-10-15T09:60:00",
        "2026-10-15T09:30:60",
        "2026-10-15T09:30",
        "2026-10-15T09:30:00.Z",
        "26-10-15T09:30:00",
        "",
    ] {
        assert_eq!(parse::date_time(text), None, "{text:?}");
    }
}

#[test]
fn leap_years_follow_the_gregorian_rule() {
    assert!(parse::date_time("2000-02-29T00:00:00").is_some());
    assert!(parse::date_time("1900-02-29T00:00:00").is_none());
    assert!(parse::date_time("2024-04-31T00:00:00").is_none());
}
