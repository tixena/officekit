use alloc::collections::BTreeMap;

use super::{
    Calendars, Era, array, checked_ids, day, era_days, eras, japanese_names, leap_years, months,
    render, resolve, roc_names, step,
};
use crate::generate_schema::cldr::{self, Tree};

fn tree(text: &str) -> Tree {
    cldr::read(text).unwrap()
}

fn supplemental() -> Tree {
    tree(
        "<supplementalData><calendarData>\
         <calendar type=\"gregorian\"><eras><era type=\"0\" end=\"0-12-31\"/>\
         <era type=\"1\" start=\"1-01-01\"/></eras></calendar>\
         <calendar type=\"japanese\"><inheritEras calendar=\"gregorian\"/><eras>\
         <era type=\"236\" start=\"2019-05-01\"/><era type=\"235\" start=\"1989-01-08\"/>\
         </eras></calendar>\
         <calendar type=\"islamic-tbla\"><eras><era type=\"0\" start=\"622-07-18\"/>\
         <era type=\"1\" end=\"622-07-17\"/></eras></calendar>\
         </calendarData></supplementalData>",
    )
}

/// `root` names every era in English and aliases its wide names to the abbreviated ones; `ja`
/// gives the abbreviated names of the Japanese eras.
fn main_files() -> BTreeMap<String, Tree> {
    let root = tree(
        "<ldml><dates><calendars>\
         <calendar type=\"gregorian\"><eras>\
         <eraNames><alias source=\"locale\" path=\"../eraAbbr\"/></eraNames>\
         <eraAbbr><era type=\"0\">BCE</era><era type=\"1\">CE</era></eraAbbr>\
         <eraNarrow><era type=\"0\">B</era><era type=\"1\">C</era></eraNarrow>\
         </eras></calendar>\
         <calendar type=\"japanese\"><eras>\
         <eraNames><alias source=\"locale\" path=\"../eraAbbr\"/></eraNames>\
         <eraAbbr><era type=\"235\">Heisei</era><era type=\"236\">Reiwa</era></eraAbbr>\
         <eraNarrow><era type=\"235\">H</era><era type=\"236\">R</era></eraNarrow>\
         </eras></calendar>\
         <calendar type=\"islamic-tbla\">\
         <alias source=\"locale\" path=\"../calendar[@type='islamic']\"/></calendar>\
         <calendar type=\"islamic\"><months><monthContext type=\"format\">\
         <monthWidth type=\"abbreviated\"><month type=\"1\">Muh.</month></monthWidth>\
         <monthWidth type=\"narrow\"><alias source=\"locale\" \
         path=\"../../monthContext[@type='stand-alone']/monthWidth[@type='narrow']\"/></monthWidth>\
         <monthWidth type=\"wide\"><month type=\"1\">Muharram</month></monthWidth>\
         </monthContext><monthContext type=\"stand-alone\">\
         <monthWidth type=\"narrow\"><month type=\"1\">1</month></monthWidth>\
         </monthContext></months></calendar>\
         </calendars></dates></ldml>",
    );
    let ja = tree(
        "<ldml><dates><calendars>\
         <calendar type=\"gregorian\"><eras><eraAbbr><era type=\"1\">\u{897f}\u{66a6}</era>\
         <era type=\"1\" alt=\"variant\">X</era></eraAbbr></eras></calendar>\
         <calendar type=\"japanese\"><eras><eraAbbr>\
         <era type=\"235\">\u{5e73}\u{6210}</era><era type=\"236\">\u{4ee4}\u{548c}</era>\
         </eraAbbr></eras></calendar>\
         </calendars></dates></ldml>",
    );
    BTreeMap::from([("ja".to_owned(), ja), ("root".to_owned(), root)])
}

fn chain(locale: &str) -> Vec<String> {
    cldr::chain(locale, &BTreeMap::new())
}

fn era(first: Option<(i64, i64, i64)>, year: i64, backward: bool, names: [&str; 3]) -> Era {
    Era {
        backward,
        first,
        names: names.map(str::to_owned),
        year,
    }
}

#[test]
fn a_day_is_its_year_month_and_day() {
    assert_eq!(day("1868-10-23").unwrap(), (1868, 10, 23));
    assert_eq!(day("0-12-31").unwrap(), (0, 12, 31));
    assert_eq!(day("-5-1-2").unwrap(), (-5, 1, 2));
    assert_eq!(
        day("1868-10").unwrap_err().to_string(),
        "1868-10 is not a day"
    );
}

#[test]
fn the_inherited_eras_come_before_the_calendars_own() {
    let found = era_days(&supplemental(), "japanese").unwrap();
    let kinds: Vec<(&str, &str)> = found
        .iter()
        .map(|era| (era.calendar.as_str(), era.kind.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("gregorian", "0"),
            ("gregorian", "1"),
            ("japanese", "236"),
            ("japanese", "235")
        ]
    );
    assert_eq!((found[0].start, found[0].end), (None, Some((0, 12, 31))));
    assert_eq!(
        era_days(&supplemental(), "coptic").unwrap_err().to_string(),
        "supplementalData.xml has no calendar coptic"
    );
}

#[test]
fn a_name_follows_the_chain_and_each_alias_from_its_start() {
    let main = main_files();
    let path = |calendar: &str, width: &str, era: &str| super::era_path(calendar, width, era);
    let ja = chain("ja_JP");
    assert_eq!(
        resolve(&main, &ja, &path("japanese", "eraNames", "236")).unwrap(),
        Some("\u{4ee4}\u{548c}".to_owned())
    );
    assert_eq!(
        resolve(&main, &ja, &path("japanese", "eraNarrow", "236")).unwrap(),
        Some("R".to_owned())
    );
    assert_eq!(
        resolve(&main, &ja, &path("gregorian", "eraAbbr", "1")).unwrap(),
        Some("\u{897f}\u{66a6}".to_owned())
    );
    assert_eq!(
        resolve(&main, &ja, &path("japanese", "eraNarrow", "237")).unwrap(),
        None
    );
}

#[test]
fn an_alias_that_returns_to_itself_stops_the_generation() {
    let main = BTreeMap::from([(
        "root".to_owned(),
        tree(
            "<ldml><dates><calendars><calendar type=\"a\">\
             <alias source=\"locale\" path=\"../calendar[@type='a']\"/>\
             </calendar></calendars></dates></ldml>",
        ),
    )]);
    let path = [
        step("dates", None),
        step("calendars", None),
        step("calendar", Some("a")),
        step("months", None),
    ];
    assert_eq!(
        resolve(&main, &chain("root"), &path)
            .unwrap_err()
            .to_string(),
        "the aliases of dates/calendars/calendar[a]/months do not end"
    );
}

#[test]
fn the_japanese_eras_are_sorted_and_named_as_section_18_8_31_says() {
    let found = eras(
        &supplemental(),
        &main_files(),
        &chain("ja_JP"),
        "japanese",
        japanese_names,
    )
    .unwrap();
    assert_eq!(
        found,
        [
            era(None, 0, true, ["B", "B", "BCE"]),
            era(
                Some((1, 1, 1)),
                1,
                false,
                ["C", "\u{897f}", "\u{897f}\u{66a6}"]
            ),
            era(
                Some((1989, 1, 8)),
                1989,
                false,
                ["H", "\u{5e73}", "\u{5e73}\u{6210}"]
            ),
            era(
                Some((2019, 5, 1)),
                2019,
                false,
                ["R", "\u{4ee4}", "\u{4ee4}\u{548c}"]
            ),
        ]
    );
}

#[test]
fn the_republic_of_china_writes_g_as_gg() {
    let names = ["n", "abbr", "wide"].map(str::to_owned);
    assert_eq!(
        roc_names(names),
        ["abbr", "abbr", "wide"].map(str::to_owned)
    );
}

#[test]
fn an_era_with_no_name_stops_the_generation() {
    let err = eras(
        &supplemental(),
        &BTreeMap::new(),
        &chain("ja"),
        "japanese",
        japanese_names,
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "ja, root gives the era 0 of gregorian no eraNarrow"
    );
}

#[test]
fn the_leap_years_are_the_list_in_the_description() {
    let keywords = tree(
        "<ldmlBCP47><keyword><key name=\"ca\">\
         <type name=\"islamic\" description=\"Hijri calendar\"/>\
         <type name=\"islamic-tbla\" description=\"tabular (intercalary years [2,5, 7] - x)\"/>\
         </key></keyword></ldmlBCP47>",
    );
    assert_eq!(leap_years(&keywords, "islamic-tbla").unwrap(), [2, 5, 7]);
    assert_eq!(
        leap_years(&keywords, "islamic").unwrap_err().to_string(),
        "the description of islamic lists no years"
    );
    assert_eq!(
        leap_years(&keywords, "persian").unwrap_err().to_string(),
        "calendar.xml does not describe persian"
    );
}

#[test]
fn the_month_names_follow_the_calendars_and_the_widths_aliases() {
    let path = [
        step("dates", None),
        step("calendars", None),
        step("calendar", Some("islamic-tbla")),
        step("months", None),
        step("monthContext", Some("format")),
        step("monthWidth", Some("narrow")),
        step("month", Some("1")),
    ];
    assert_eq!(
        resolve(&main_files(), &chain("en"), &path).unwrap(),
        Some("1".to_owned())
    );
    let err = months(&main_files(), &chain("en"), "islamic-tbla").unwrap_err();
    assert_eq!(
        err.to_string(),
        "en, root gives month 2 of islamic-tbla no narrow name"
    );
}

#[test]
fn each_language_id_once_and_ja_jp_and_zh_tw_named() {
    let ids = vec![
        (0x411, "ja-jp".to_owned()),
        (0x404, "zh-tw".to_owned()),
        (0x411, "ja-jp".to_owned()),
    ];
    assert_eq!(
        checked_ids(ids).unwrap(),
        [(0x404, "zh-tw".to_owned()), (0x411, "ja-jp".to_owned())]
    );
    let twice = vec![(0x411, "ja-jp".to_owned()), (0x411, "ko-kr".to_owned())];
    assert_eq!(
        checked_ids(twice).unwrap_err().to_string(),
        "section 18.8.30 gives the language id 411 to ja-jp and to ko-kr"
    );
    assert_eq!(
        checked_ids(vec![(0x411, "ja-jp".to_owned())])
            .unwrap_err()
            .to_string(),
        "section 18.8.30 gives zh-tw no language id"
    );
}

#[test]
fn an_array_goes_on_one_line_when_it_fits() {
    let short = ["\"a\"".to_owned(), "\"b\"".to_owned()];
    assert_eq!(array(&short, 8), "[\"a\", \"b\"]");
    let long = ["x".repeat(30), "y".repeat(30)];
    assert_eq!(
        array(&long, 4),
        format!(
            "[\n        {},\n        {},\n    ]",
            "x".repeat(30),
            "y".repeat(30)
        )
    );
}

#[test]
fn the_table_holds_each_era_the_hijri_calendar_and_the_language_ids() {
    let calendars = Calendars {
        hijri_first: (622, 7, 18),
        hijri_leap_years: vec![2, 5],
        hijri_months: vec![["1", "Muh.", "Muharram"].map(str::to_owned)],
        japanese: vec![era(
            Some((2019, 5, 1)),
            2019,
            false,
            ["R", "\u{4ee4}", "\u{4ee4}\u{548c}"],
        )],
        language_ids: vec![(0x411, "ja-jp".to_owned())],
        roc: vec![era(None, 1911, true, ["a", "a", "b"])],
    };
    let text = render("// header\n", "Copyright", &calendars);
    assert!(
        text.starts_with(
            "// header\n// Derived from Unicode data; the Unicode License v3 notice is in the \
             crate's LICENSE-UNICODE.\n\n//! Unicode CLDR 49's eras"
        ),
        "{text}"
    );
    for expected in [
        "//! Copyright Unicode License v3.",
        "        backward: false,\n        first: days_from_civil(2019, 5, 1),\n        \
         names: [\"R\", \"\\u{4ee4}\", \"\\u{4ee4}\\u{548c}\"],\n        year: 2019,",
        "        backward: true,\n        first: i64::MIN,",
        "pub(super) const HIJRI_FIRST: i64 = days_from_civil(622, 7, 18);",
        "pub(super) static HIJRI_LEAP_YEARS: [i64; 2] = [2, 5];",
        "    [\"1\", \"Muh.\", \"Muharram\"],",
        "pub(super) static LANGUAGE_IDS: [(u16, &str); 1] = [(0x0411, \"ja-jp\")];",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }
}
