use officekit_xml::values::DateTime;

use super::super::{FormatColor, NumberFormat, Piece};
use super::{Date, DateSystem, FIRST, LAST, date_time, days_from_civil};

const STRICT: DateSystem = DateSystem::V1900;
const TRANSITIONAL: DateSystem = DateSystem::V1900Transitional;
const V1904: DateSystem = DateSystem::V1904;

/// `serial` under `code` in `system`, as one text.
fn shown(code: &str, serial: &str, system: DateSystem) -> Option<String> {
    NumberFormat::parse(code)
        .unwrap()
        .number(serial, system)
        .map(|written| written.plain())
}

/// A date and a time with no zone.
const fn at(date: (i32, u8, u8), time: (u8, u8, u8, u32)) -> DateTime {
    DateTime::new(date, time, None)
}

/// The date `serial` gives in `system`, at midnight.
fn day_of(serial: &str, system: DateSystem) -> Option<(i32, u8, u8)> {
    let found = date_time(serial, system)?;
    assert_eq!(
        (found.hour, found.minute, found.second, found.nanosecond),
        (0, 0, 0, 0)
    );
    Some((found.year, found.month, found.day))
}

#[test]
fn the_strict_1900_system_counts_from_1899_12_30() {
    assert_eq!(
        date_time("1.5", STRICT),
        Some(at((1899_i32, 12, 31), (12, 0, 0, 0)))
    );
    assert_eq!(
        date_time("2.5", STRICT),
        Some(at((1900_i32, 1, 1), (12, 0, 0, 0)))
    );
    assert_eq!(
        date_time("-4.25", STRICT),
        Some(at((1899_i32, 12, 25), (18, 0, 0, 0)))
    );
    assert_eq!(
        date_time("-1.25", STRICT),
        Some(at((1899_i32, 12, 28), (18, 0, 0, 0)))
    );
    assert_eq!(
        date_time("3687.4207639", STRICT),
        Some(at((1910_i32, 2, 3), (10, 5, 54, 960_000)))
    );
    assert_eq!(
        date_time("-2337.999989", STRICT),
        Some(at((1893_i32, 8, 5), (0, 0, 0, 950_400_000)))
    );
    let code = "yyyy-mm-dd hh:mm:ss";
    assert_eq!(
        shown(code, "3687.4207639", STRICT).as_deref(),
        Some("1910-02-03 10:05:54")
    );
    assert_eq!(
        shown(code, "-2337.999989", STRICT).as_deref(),
        Some("1893-08-05 00:00:01")
    );
    assert_eq!(
        shown(code, "2958465.9999884", STRICT).as_deref(),
        Some("9999-12-31 23:59:59")
    );
}

#[test]
fn the_transitional_1900_system_counts_day_1_as_1900_01_01() {
    assert_eq!(day_of("1", TRANSITIONAL), Some((1900_i32, 1, 1)));
    assert_eq!(day_of("59", TRANSITIONAL), Some((1900_i32, 2, 28)));
    assert_eq!(day_of("61", TRANSITIONAL), Some((1900_i32, 3, 1)));
    assert_eq!(day_of("45658", TRANSITIONAL), Some((2025_i32, 1, 1)));
    assert_eq!(day_of("46000", TRANSITIONAL), Some((2025_i32, 12, 9)));
    assert_eq!(day_of("2958465", TRANSITIONAL), Some((9999_i32, 12, 31)));
}

#[test]
fn day_60_and_the_days_below_1_of_a_transitional_file_are_no_date() {
    for serial in ["60", "60.5", "0.5", "-1"] {
        assert_eq!(date_time(serial, TRANSITIONAL), None, "{serial}");
        assert_eq!(shown("yyyy-mm-dd", serial, TRANSITIONAL), None, "{serial}");
    }
    assert_eq!(
        shown("h:mm", "60.5", TRANSITIONAL).as_deref(),
        Some("12:00")
    );
    assert_eq!(shown("h:mm", "0.5", TRANSITIONAL).as_deref(), Some("12:00"));
    assert_eq!(shown("h:mm", "-0.5", TRANSITIONAL), None);
}

#[test]
fn the_1904_system_counts_from_1904_01_01() {
    assert_eq!(day_of("0", V1904), Some((1904_i32, 1, 1)));
    assert_eq!(day_of("1", V1904), Some((1904_i32, 1, 2)));
    assert_eq!(day_of("-695055", V1904), Some((1_i32, 1, 1)));
    assert_eq!(day_of("2957003", V1904), Some((9999_i32, 12, 31)));
    assert_eq!(
        date_time("0.5", V1904),
        Some(at((1904_i32, 1, 1), (12, 0, 0, 0)))
    );
    let code = "yyyy-mm-dd hh:mm:ss";
    for (serial, expected) in [
        ("-3799.999989", "1893-08-05 00:00:01"),
        ("2225.4207639", "1910-02-03 10:05:54"),
        ("2957003.9999884", "9999-12-31 23:59:59"),
    ] {
        assert_eq!(shown(code, serial, V1904).as_deref(), Some(expected));
    }
}

#[test]
fn a_serial_date_time_outside_its_systems_limits_is_no_date() {
    assert_eq!(day_of("-693593", STRICT), Some((1_i32, 1, 1)));
    assert_eq!(day_of("2958465", STRICT), Some((9999_i32, 12, 31)));
    assert_eq!(date_time("-693594", STRICT), None);
    assert_eq!(date_time("-693593.5", STRICT), None);
    assert_eq!(date_time("2958466", STRICT), None);
    assert_eq!(date_time("-695056", V1904), None);
    assert_eq!(date_time("2957004", V1904), None);
    assert_eq!(date_time("2958466", TRANSITIONAL), None);
    assert_eq!(date_time("1e300", STRICT), None);
    assert_eq!(shown("h:mm", "2958466", STRICT), None);
    assert_eq!(shown("[h]", "2957004", V1904), None);
}

#[test]
fn the_standards_times_of_day() {
    for (serial, expected) in [
        ("0", "00:00:00"),
        ("0.0000115", "00:00:01"),
        ("0.4207639", "10:05:54"),
        ("0.5", "12:00:00"),
        ("0.9999884", "23:59:59"),
    ] {
        for system in [STRICT, TRANSITIONAL, V1904] {
            assert_eq!(shown("hh:mm:ss", serial, system).as_deref(), Some(expected));
        }
    }
    assert_eq!(
        date_time("0.0000115", STRICT),
        Some(at((1899_i32, 12, 30), (0, 0, 0, 993_600_000)))
    );
    assert_eq!(
        date_time("4.66666667", STRICT),
        Some(at((1900_i32, 1, 3), (16, 0, 0, 288_000)))
    );
}

#[test]
fn a_negative_serial_date_time_is_in_the_day_below_it() {
    assert_eq!(
        date_time("-2.00000001", STRICT),
        Some(at((1899_i32, 12, 27), (23, 59, 59, 999_136_000)))
    );
}

#[test]
fn what_is_no_number_is_no_date() {
    for serial in ["abc", "INF", ""] {
        assert_eq!(date_time(serial, STRICT), None, "{serial}");
        assert_eq!(shown("yyyy-mm-dd", serial, STRICT), None, "{serial}");
    }
}

#[test]
fn the_month_codes() {
    for (code, expected) in [
        ("m", "9"),
        ("mm", "09"),
        ("mmm", "Sep"),
        ("mmmm", "September"),
        ("mmmmm", "S"),
    ] {
        assert_eq!(
            shown(code, "45909", TRANSITIONAL).as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn the_day_and_year_codes() {
    for (code, expected) in [
        ("d", "9"),
        ("dd", "09"),
        ("ddd", "Tue"),
        ("dddd", "Tuesday"),
        ("yy", "25"),
        ("yyyy", "2025"),
        ("e", "2025"),
        ("ee", "25"),
    ] {
        assert_eq!(
            shown(code, "46000", TRANSITIONAL).as_deref(),
            Some(expected),
            "{code}"
        );
    }
}

#[test]
fn the_built_in_codes_of_dates() {
    for (code, expected) in [
        ("mm-dd-yy", "12-09-25"),
        ("d-mmm-yy", "9-Dec-25"),
        ("d-mmm", "9-Dec"),
        ("mmm-yy", "Dec-25"),
        ("m/d/yy h:mm", "12/9/25 10:05"),
    ] {
        assert_eq!(
            shown(code, "46000.4207639", TRANSITIONAL).as_deref(),
            Some(expected),
            "{code}"
        );
    }
}

#[test]
fn m_is_the_minute_after_an_hour_or_before_a_second_and_the_month_elsewhere() {
    for (code, expected) in [
        ("h:mm", "10:05"),
        ("mm:ss", "05:54"),
        ("m/d h:m", "12/9 10:5"),
        ("hh \"h\" mm", "10 h 05"),
        ("[h]:mm", "1104010:05"),
        ("mm", "12"),
        ("mm h", "12 10"),
        ("h mmm", "10 Dec"),
    ] {
        assert_eq!(
            shown(code, "46000.4207639", TRANSITIONAL).as_deref(),
            Some(expected),
            "{code}"
        );
    }
}

#[test]
fn literals_and_a_colour_in_a_section_of_dates() {
    let written = NumberFormat::parse("[Red]yyyy\"-\"mm")
        .unwrap()
        .number("46000", TRANSITIONAL)
        .unwrap();
    assert_eq!(written.plain(), "2025-12");
    assert_eq!(written.color, Some(FormatColor::Red));
    let filled = NumberFormat::parse("d, yyyy_)* ")
        .unwrap()
        .number("46000", TRANSITIONAL)
        .unwrap();
    assert_eq!(
        filled.pieces,
        [
            Piece::Text("9, 2025".to_owned()),
            Piece::Skip(')'),
            Piece::Fill(' ')
        ]
    );
}

#[test]
fn am_pm_writes_the_half_of_the_day_in_the_codes_case() {
    assert_eq!(
        shown("h:mm AM/PM", "0.5", TRANSITIONAL).as_deref(),
        Some("12:00 PM")
    );
    assert_eq!(
        shown("h:mm:ss A/P", "0.0", TRANSITIONAL).as_deref(),
        Some("12:00:00 A")
    );
    assert_eq!(
        shown("hh:mm am/pm", "0.75", TRANSITIONAL).as_deref(),
        Some("06:00 pm")
    );
    assert_eq!(
        shown("h:mm AM/PM", "0.4207639", TRANSITIONAL).as_deref(),
        Some("10:05 AM")
    );
}

#[test]
fn with_no_am_pm_the_hour_runs_to_23() {
    assert_eq!(
        shown("h:mm", "0.75", TRANSITIONAL).as_deref(),
        Some("18:00")
    );
    assert_eq!(
        shown("hh:mm:ss", "0.75", TRANSITIONAL).as_deref(),
        Some("18:00:00")
    );
}

#[test]
fn counts_in_brackets_run_past_24_and_60() {
    assert_eq!(
        shown("[h]:mm", "1.0430556", TRANSITIONAL).as_deref(),
        Some("25:02")
    );
    assert_eq!(
        shown("[mm]:ss", "0.0432407", TRANSITIONAL).as_deref(),
        Some("62:16")
    );
    assert_eq!(
        shown("[ss].00", "0.0432384259", TRANSITIONAL).as_deref(),
        Some("3735.80")
    );
}

#[test]
fn decimals_of_a_second() {
    assert_eq!(
        shown("h:mm:ss.00", "0.6917535", TRANSITIONAL).as_deref(),
        Some("16:36:07.50")
    );
    assert_eq!(
        shown("h:mm:ss.0", "0.6917535", TRANSITIONAL).as_deref(),
        Some("16:36:07.5")
    );
    assert_eq!(
        shown("h:mm:ss.000", "0.6917535", TRANSITIONAL).as_deref(),
        Some("16:36:07.502")
    );
}

#[test]
fn an_instant_is_rounded_to_the_second_and_what_is_not_shown_is_dropped() {
    assert_eq!(
        shown("h:mm:ss", "0.9999884", TRANSITIONAL).as_deref(),
        Some("23:59:59")
    );
    assert_eq!(
        shown("h:mm", "0.9999884", TRANSITIONAL).as_deref(),
        Some("23:59")
    );
    assert_eq!(
        shown("yyyy-mm-dd h:mm:ss", "46000.999999", TRANSITIONAL).as_deref(),
        Some("2025-12-10 0:00:00")
    );
}

#[test]
fn the_standards_table_of_times() {
    for (code, serial, expected) in [
        ("h AM/PM", "0.166666666667", "4 AM"),
        ("h:mm AM/PM", "0.691666666667", "4:36 PM"),
        ("h:mm:ss A/P", "0.691701388889", "4:36:03 P"),
        ("h:mm:ss.00", "0.19171006944444", "4:36:03.75"),
    ] {
        assert_eq!(
            shown(code, serial, TRANSITIONAL).as_deref(),
            Some(expected),
            "{code}"
        );
    }
}

#[test]
fn a_code_of_numbers_reads_no_date_system() {
    for system in [STRICT, TRANSITIONAL, V1904] {
        assert_eq!(shown("0.00", "1234.5", system).as_deref(), Some("1234.50"));
        assert_eq!(shown("0.00", "-1", system).as_deref(), Some("-1.00"));
    }
}

#[test]
fn each_day_of_the_limits_turns_back_into_its_count() {
    for days in FIRST..=LAST {
        let date = Date::from_days(days);
        assert_eq!(days_from_civil(date.year, date.month, date.day), days);
    }
    assert_eq!(Date::from_days(0).weekday_name(), "Thursday");
}
