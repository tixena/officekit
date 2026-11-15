use officekit_xml::values::DateTime;

use super::super::{FormatColor, NumberFormat, Piece};
use super::{Date, DateSystem, FIRST, HIJRI_FIRST, LAST, Locale, date_time, days_from_civil};

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

/// `serial` under `code` in the Strict 1900 system, in `language`, as one text.
fn shown_in(code: &str, serial: &str, language: Option<&str>) -> Option<String> {
    NumberFormat::parse(code)
        .unwrap()
        .number_in(serial, STRICT, language)
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

#[test]
fn reiwa_6_is_2024_under_the_japanese_eras() {
    let reiwa = "\u{4ee4}\u{548c}";
    let code = "[$-411]ggge\"\u{5e74}\"m\"\u{6708}\"d\"\u{65e5}\"";
    assert_eq!(
        shown_in(code, "45413", None),
        Some(format!("{reiwa}6\u{5e74}5\u{6708}1\u{65e5}"))
    );
    assert_eq!(
        shown_in("ggge\"\u{5e74}\"", "45413", Some("ja-JP")),
        Some(format!("{reiwa}6\u{5e74}"))
    );
}

#[test]
fn heisei_31_ends_on_2019_04_30_and_reiwa_1_starts_the_next_day() {
    for (code, heisei, reiwa) in [
        ("[$-411]ggge", "\u{5e73}\u{6210}31", "\u{4ee4}\u{548c}1"),
        ("[$-411]gge", "\u{5e73}31", "\u{4ee4}1"),
        ("[$-411]ge.m.d", "H31.4.30", "R1.5.1"),
        ("[$-411]ee", "31", "01"),
    ] {
        assert_eq!(
            shown_in(code, "43585", None).as_deref(),
            Some(heisei),
            "{code}"
        );
        assert_eq!(
            shown_in(code, "43586", None).as_deref(),
            Some(reiwa),
            "{code}"
        );
    }
    assert_eq!(shown_in("[$-411]ge", "32515", None).as_deref(), Some("S64"));
    assert_eq!(shown_in("[$-411]ge", "32516", None).as_deref(), Some("H1"));
}

#[test]
fn before_meiji_the_japanese_eras_are_the_gregorian_ones_cldr_inherits() {
    assert_eq!(shown_in("[$-411]ge", "-11390", None).as_deref(), Some("M1"));
    assert_eq!(
        shown_in("[$-411]ge", "-11391", None).as_deref(),
        Some("AD1868")
    );
}

#[test]
fn r_is_ee_and_rr_is_gggee_under_ja_jp() {
    assert_eq!(shown_in("[$-411]r", "45413", None).as_deref(), Some("06"));
    assert_eq!(
        shown_in("[$-411]rr", "45413", None).as_deref(),
        Some("\u{4ee4}\u{548c}06")
    );
}

#[test]
fn the_republic_of_china_year_under_zh_tw() {
    let republic = "\u{6c11}\u{570b}";
    assert_eq!(
        shown_in("[$-404]e/m/d", "45413", None).as_deref(),
        Some("113/5/1")
    );
    assert_eq!(shown_in("[$-404]ee", "4384", None).as_deref(), Some("01"));
    for code in ["[$-404]ge", "[$-404]gge", "[$-404]ggge"] {
        assert_eq!(
            shown_in(code, "45413", None),
            Some(format!("{republic}113")),
            "{code}"
        );
    }
    for code in ["[$-404]r", "[$-404]rr"] {
        assert_eq!(
            shown_in(code, "45413", None).as_deref(),
            Some("113"),
            "{code}"
        );
    }
}

#[test]
fn before_1912_zh_tw_counts_back_after_a_g_and_writes_the_gregorian_year_else() {
    assert_eq!(shown_in("[$-404]e", "4383", None).as_deref(), Some("1911"));
    assert_eq!(
        shown_in("[$-404]ge", "4383", None),
        Some("\u{6c11}\u{570b}\u{524d}1".to_owned())
    );
    assert_eq!(shown_in("[$-404]e", "4384", None).as_deref(), Some("1"));
}

#[test]
fn the_codes_bracket_names_the_language_before_the_callers() {
    assert_eq!(shown_in("ge", "45413", Some("ja")).as_deref(), Some("R6"));
    assert_eq!(
        shown_in("ge", "45413", Some("zh-Hant")).as_deref(),
        Some("\u{6c11}\u{570b}113")
    );
    assert_eq!(
        shown_in("[$-409]ge", "45413", Some("ja-JP")).as_deref(),
        Some("2024")
    );
    assert_eq!(
        shown_in("[$-404]e", "45413", Some("ja-JP")).as_deref(),
        Some("113")
    );
    assert_eq!(
        shown_in("[$-30411]ge", "45413", None).as_deref(),
        Some("R6")
    );
    assert_eq!(
        shown_in("[$-x-sysdate]ge", "45413", Some("ja")).as_deref(),
        Some("R6")
    );
}

#[test]
fn other_locales_write_no_era_and_e_as_yyyy() {
    for language in [None, Some("en-US"), Some("zh-CN"), Some("zh-Hant-HK")] {
        for (code, expected) in [("ggge", "2024"), ("gg ee", " 24"), ("r", ""), ("rr", "")] {
            assert_eq!(
                shown_in(code, "45413", language).as_deref(),
                Some(expected),
                "{code} {language:?}"
            );
        }
    }
    assert_eq!(shown("ggg", "60", TRANSITIONAL).as_deref(), Some(""));
}

#[test]
fn the_locale_of_a_language_tag() {
    for (tag, expected) in [
        ("ja", Some(Locale::Japan)),
        ("JA-jp", Some(Locale::Japan)),
        ("ja_JP", Some(Locale::Japan)),
        ("zh-TW", Some(Locale::Taiwan)),
        ("zh-Hant", Some(Locale::Taiwan)),
        ("zh-Hans-TW", Some(Locale::Taiwan)),
        ("zh", None),
        ("zh-Hant-HK", None),
        ("ja-US", None),
        ("", None),
    ] {
        assert_eq!(Locale::of(tag), expected, "{tag}");
    }
}

#[test]
fn b2_writes_the_standards_hijri_date_of_2007_12_17() {
    // Part 1, section L.1.18.5 writes 2007-12-17 as 8 Thoul Hijjah 1428.
    assert_eq!(
        shown("B2yyyy/mm/dd", "39433", STRICT).as_deref(),
        Some("1428/12/08")
    );
    assert_eq!(
        shown("b2d mmmm yyyy", "39433", STRICT).as_deref(),
        Some("8 Dhu\u{2bb}l-Hijjah 1428")
    );
    assert_eq!(
        shown("b2d mmm, mmmmm e dddd", "39433", STRICT).as_deref(),
        Some("8 Dhu\u{2bb}l-H., 12 1428 Monday")
    );
    assert_eq!(
        shown("b2b1yyyy-m-d", "39433", STRICT).as_deref(),
        Some("2007-12-17")
    );
    assert_eq!(
        shown("[$-411]b2ge", "39433", STRICT).as_deref(),
        Some("1428")
    );
}

#[test]
fn b2_finds_no_date_before_the_first_day_of_the_hijri_calendar() {
    assert_eq!(
        shown("b2yyyy-mm-dd", "-466580", STRICT).as_deref(),
        Some("0001-01-01")
    );
    assert_eq!(shown("b2yyyy-mm-dd", "-466581", STRICT), None);
    assert_eq!(shown("b2h:mm", "-466581", STRICT).as_deref(), Some("0:00"));
    assert_eq!(
        shown("yyyy-mm-dd", "-466581", STRICT).as_deref(),
        Some("0622-07-17")
    );
}

/// Days from 1970-01-01 to `day` of `month` of `year` of the tabular Hijri calendar, by its
/// closed form: 30 days to each odd month, 29 to each even one, 11 leap days in 30 years.
const fn hijri_days(year: i64, month: i64, day: i64) -> i64 {
    HIJRI_FIRST - 1 + (year - 1) * 354 + (3 + 11 * year) / 30 + 29 * (month - 1) + month / 2 + day
}

#[test]
fn each_hijri_day_of_the_limits_turns_back_into_its_count() {
    for days in HIJRI_FIRST..=LAST {
        let date = Date::hijri(days).unwrap();
        assert_eq!(hijri_days(date.year, date.month, date.day), days);
    }
    assert!(Date::hijri(HIJRI_FIRST - 1).is_none());
}
