use alloc::collections::BTreeMap;

use super::{Formats, Names, formats, month_lists, names, pruned, render, weekday_lists};
use crate::generate_schema::cldr::{self, Tree};

fn tree(text: &str) -> Tree {
    cldr::read(text).unwrap()
}

/// `count` names, each `prefix` and its number.
fn numbered(prefix: &str, count: usize) -> String {
    (1..=count)
        .map(|at| format!("<month type=\"{at}\">{prefix}{at}</month>"))
        .collect::<Vec<String>>()
        .concat()
}

fn days(prefix: &str) -> String {
    ["sun", "mon", "tue", "wed", "thu", "fri", "sat"]
        .map(|day| format!("<day type=\"{day}\">{prefix}{day}</day>"))
        .concat()
}

/// `root` gives every name and aliases the islamic-tbla calendar to islamic and the narrow months
/// to the abbreviated ones; `es` gives its wide months and weekdays and its long date pattern;
/// `es_MX`'s parent is `es_419`, which gives its abbreviated weekdays.
fn main_files() -> BTreeMap<String, Tree> {
    let formats = "<dateFormats><dateFormatLength type=\"long\"><dateFormat><pattern>y MMMM d\
                   </pattern></dateFormat></dateFormatLength></dateFormats><timeFormats>\
                   <timeFormatLength type=\"medium\"><timeFormat><pattern>HH:mm:ss</pattern>\
                   <pattern alt=\"ascii\">h:mm:ss</pattern></timeFormat></timeFormatLength>\
                   </timeFormats><dayPeriods><dayPeriodContext type=\"format\">\
                   <dayPeriodWidth type=\"abbreviated\"><dayPeriod type=\"am\">AM</dayPeriod>\
                   <dayPeriod type=\"pm\">PM</dayPeriod></dayPeriodWidth></dayPeriodContext>\
                   </dayPeriods>";
    let months = |calendar: &str, prefix: &str| {
        format!(
            "<calendar type=\"{calendar}\"><months><monthContext type=\"format\">\
             <monthWidth type=\"abbreviated\">{}</monthWidth>\
             <monthWidth type=\"narrow\"><alias source=\"locale\" \
             path=\"../monthWidth[@type='abbreviated']\"/></monthWidth>\
             <monthWidth type=\"wide\">{}</monthWidth>\
             </monthContext></months>\
             <days><dayContext type=\"format\"><dayWidth type=\"abbreviated\">{}</dayWidth>\
             <dayWidth type=\"wide\">{}</dayWidth></dayContext></days>{}</calendar>",
            numbered(&format!("{prefix}a"), 12),
            numbered(&format!("{prefix}w"), 12),
            days(&format!("{prefix}a")),
            days(&format!("{prefix}w")),
            if calendar == "gregorian" { formats } else { "" },
        )
    };
    let root = tree(&format!(
        "<ldml><identity/><dates><calendars>{}{}\
         <calendar type=\"islamic-tbla\"><alias source=\"locale\" \
         path=\"../calendar[@type='islamic']\"/></calendar>\
         </calendars><fields/></dates></ldml>",
        months("gregorian", "r"),
        months("islamic", "h"),
    ));
    let es = tree(&format!(
        "<ldml><dates><calendars><calendar type=\"gregorian\"><months>\
         <monthContext type=\"format\"><monthWidth type=\"wide\">{}</monthWidth></monthContext>\
         <monthContext type=\"stand-alone\"><monthWidth type=\"wide\">{}</monthWidth></monthContext>\
         </months><days><dayContext type=\"format\"><dayWidth type=\"wide\">{}</dayWidth>\
         </dayContext></days><eras/><dateFormats><dateFormatLength type=\"long\"><dateFormat>\
         <pattern>d 'de' MMMM 'de' y</pattern></dateFormat></dateFormatLength></dateFormats>\
         </calendar></calendars></dates></ldml>",
        numbered("es", 12),
        numbered("ES", 12),
        days("es"),
    ));
    let es_419 = tree(&format!(
        "<ldml><dates><calendars><calendar type=\"gregorian\"><days><dayContext type=\"format\">\
         <dayWidth type=\"abbreviated\">{}</dayWidth></dayContext></days></calendar>\
         </calendars></dates></ldml>",
        days("419"),
    ));
    BTreeMap::from([
        ("en".to_owned(), tree("<ldml/>")),
        ("es".to_owned(), es),
        ("es_419".to_owned(), es_419),
        ("es_MX".to_owned(), tree("<ldml/>")),
        ("root".to_owned(), root),
    ])
}

fn parents() -> BTreeMap<String, String> {
    BTreeMap::from([("es_MX".to_owned(), "es_419".to_owned())])
}

fn list(prefix: &str, count: usize) -> Vec<String> {
    (1..=count).map(|at| format!("{prefix}{at}")).collect()
}

fn day_list(prefix: &str) -> Vec<String> {
    ["sun", "mon", "tue", "wed", "thu", "fri", "sat"]
        .map(|day| format!("{prefix}{day}"))
        .to_vec()
}

#[test]
fn pruning_keeps_each_calendars_names_formats_and_aliases() {
    let file = tree(
        "<ldml><identity/><dates><calendars><alias source=\"locale\" path=\"x\"/>\
         <calendar type=\"gregorian\"><months/><days/><eras/><dateFormats/><timeFormats/>\
         <dayPeriods/><dateTimeFormats/><alias source=\"locale\" path=\"y\"/></calendar></calendars><fields/></dates><numbers/></ldml>",
    );
    let kept = pruned(file, 0);
    let dates = kept.child("dates").unwrap();
    assert_eq!(kept.children.len(), 1);
    assert_eq!(dates.children.len(), 1);
    let calendars = dates.child("calendars").unwrap();
    assert_eq!(calendars.children.len(), 2);
    let names: Vec<&str> = calendars
        .child("calendar")
        .unwrap()
        .children
        .iter()
        .map(|child| child.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "months",
            "days",
            "dateFormats",
            "timeFormats",
            "dayPeriods",
            "alias"
        ]
    );
}

#[test]
fn the_names_are_the_format_contexts_through_the_parents_and_the_aliases() {
    let main = main_files();
    let es = cldr::chain("es_MX", &parents());
    assert_eq!(
        month_lists(&main, &es, "gregorian").unwrap(),
        [list("ra", 12), list("ra", 12), list("es", 12)]
    );
    assert_eq!(
        month_lists(&main, &es, "islamic-tbla").unwrap(),
        [list("ha", 12), list("ha", 12), list("hw", 12)]
    );
    assert_eq!(
        weekday_lists(&main, &es).unwrap(),
        [day_list("419"), day_list("es")]
    );
    assert_eq!(
        formats(&main, &es).unwrap(),
        Formats {
            date: "d 'de' MMMM 'de' y".to_owned(),
            halves: vec!["AM".to_owned(), "PM".to_owned()],
            time: "HH:mm:ss".to_owned(),
        }
    );
    let none = BTreeMap::from([("root".to_owned(), tree("<ldml/>"))]);
    assert_eq!(
        formats(&none, &cldr::chain("es", &BTreeMap::new()))
            .unwrap_err()
            .to_string(),
        "es, root gives no am dayPeriods"
    );
    assert_eq!(
        weekday_lists(&none, &cldr::chain("es", &BTreeMap::new()))
            .unwrap_err()
            .to_string(),
        "es, root gives sun no abbreviated name"
    );
}

#[test]
fn each_locale_but_root_shares_the_lists_it_gives_alike() {
    let found = names(&main_files(), &parents()).unwrap();
    let locales: Vec<(&str, [usize; 8])> = found
        .locales
        .iter()
        .map(|(locale, lists)| (locale.as_str(), *lists))
        .collect();
    assert_eq!(
        locales,
        [
            ("en", [0, 0, 1, 0, 1, 2, 2, 3]),
            ("es", [0, 0, 4, 0, 2, 2, 2, 3]),
            ("es_419", [0, 0, 4, 3, 2, 2, 2, 3]),
            ("es_MX", [0, 0, 4, 3, 2, 2, 2, 3]),
        ]
    );
    assert_eq!(found.months.len(), 5);
    assert_eq!(found.weekdays.len(), 4);
    assert_eq!(found.system, [[0, 1, 0], [2, 1, 0], [2, 1, 0], [2, 1, 0]]);
    assert_eq!(
        found.patterns,
        ["y MMMM d", "HH:mm:ss", "d 'de' MMMM 'de' y"]
    );
    let mut without = main_files();
    without.remove("en");
    assert_eq!(
        names(&without, &parents()).unwrap_err().to_string(),
        "CLDR has no locale en"
    );
}

#[test]
fn the_table_holds_the_lists_the_locales_and_their_indexes() {
    let mut found = Names::default();
    let months = ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"]
        .map(str::to_owned)
        .to_vec();
    let wide = list("enero", 12);
    let days = day_list("d");
    found.add(
        "es",
        [months.clone(), months.clone(), wide.clone()],
        [days.clone(), days],
        [months.clone(), months, wide],
        Formats {
            date: "d 'de' MMMM 'de' y".to_owned(),
            halves: vec!["a. m.".to_owned(), "p. m.".to_owned()],
            time: "H:mm:ss".to_owned(),
        },
    );
    let text = render("// header\n", "Copyright", &found).unwrap();
    assert!(
        text.starts_with(
            "// header\n// Derived from Unicode data; the Unicode License v3 notice is in the \
             crate's LICENSE-UNICODE.\n\n//! Unicode CLDR 49's month and weekday names"
        ),
        "{text}"
    );
    for expected in [
        "pub(super) static MONTHS: [[&str; 12]; 2] = [\n    [\"J\", \"F\", \"M\", \"A\", \"M\", \
         \"J\", \"J\", \"A\", \"S\", \"O\", \"N\", \"D\"],\n    [\n        \"enero1\",",
        "pub(super) static WEEKDAYS: [[&str; 7]; 1] = [\n    [\"dsun\", \"dmon\", \"dtue\", \
         \"dwed\", \"dthu\", \"dfri\", \"dsat\"],\n];\n",
        "pub(super) static LOCALES: [&str; 1] = [\"es\"];",
        "pub(super) static NAMES: [[u16; 8]; 1] = [\n    [0, 0, 1, 0, 0, 0, 0, 1],\n];\n",
        "pub(super) static PATTERNS: [&str; 2] = [\"d 'de' MMMM 'de' y\", \"H:mm:ss\"];",
        "pub(super) static HALVES: [[&str; 2]; 1] = [\n    [\"a. m.\", \"p. m.\"],\n];\n",
        "pub(super) static SYSTEM: [[u16; 3]; 1] = [\n    [0, 1, 0],\n];\n",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }
}
