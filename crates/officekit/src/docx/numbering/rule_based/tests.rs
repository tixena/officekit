use alloc::string::String;
use alloc::vec::Vec;

use officekit_markup::wordprocessing::StNumberFormatMember as F;

use super::{
    Category, FORMATS, LOCALES, PARENTS, Pattern, PluralKind, Relation, Spelled, WORDS, Words,
    counted, dollars, locale, normalized, spelled,
};

/// Words with Latin digits, `.` between groups and `,` before a fraction.
const GROUPED: Words = Words {
    and: "{0} y {1}",
    cardinal: None,
    cardinal_plurals: 0,
    decimal: ",",
    digits: "0123456789",
    group: ".",
    ordinal: None,
    ordinal_plurals: 0,
    ordinal_text: None,
};

fn pattern(minimum: u8, primary: u8, secondary: u8) -> Pattern {
    Pattern {
        fraction: 0,
        minimum,
        point: false,
        primary,
        secondary,
    }
}

#[test]
fn each_locale_with_words_writes_the_values_of_the_acceptance_list() {
    for found in &LOCALES {
        let Some(words) = found.words.and_then(|at| WORDS.get(at)) else {
            continue;
        };
        let language = found.id;
        for value in [1, 2, 3, 11, 21, 100] {
            assert!(
                spelled(value, Spelled::Cardinal, language).is_some(),
                "{language} {value}"
            );
            assert!(
                spelled(value, Spelled::Ordinal, language).is_some(),
                "{language} {value}"
            );
            assert!(dollars(value, language).is_some(), "{language} {value}");
            assert_eq!(
                spelled(value, Spelled::OrdinalText, language).is_some(),
                words.ordinal_text.is_some(),
                "{language} {value}"
            );
        }
    }
}

#[test]
fn the_tables_are_sorted_for_their_searches() {
    assert!(LOCALES.is_sorted_by(|left, right| left.id < right.id));
    assert!(PARENTS.is_sorted_by(|left, right| left.0 < right.0));
}

#[test]
fn a_tag_becomes_a_cldr_id() {
    assert_eq!(normalized("en-US").as_deref(), Some("en_US"));
    assert_eq!(normalized("SR-latn-rs").as_deref(), Some("sr_Latn_RS"));
    assert_eq!(normalized("es-419").as_deref(), Some("es_419"));
    assert_eq!(normalized("de-DE-1996-x-a").as_deref(), Some("de_DE"));
    assert_eq!(normalized("x-none").as_deref(), None);
    assert_eq!(normalized("0409").as_deref(), None);
    assert_eq!(normalized("").as_deref(), None);
}

#[test]
fn a_locale_is_found_through_its_parents_up_to_root() {
    assert_eq!(locale("sr-Latn-RS").map(|found| found.id), Some("sr_Latn"));
    assert_eq!(
        locale("es-AR").map(|found| found.id).unwrap().get(..2),
        Some("es")
    );
    assert!(locale("az-Cyrl-AZ").is_none_or(|found| found.words.is_none()));
    assert!(locale("zz").is_none());
}

#[test]
fn digits_are_grouped_as_the_pattern_says() {
    assert_eq!(GROUPED.digits(1_234_567, &pattern(1, 3, 3)), "1.234.567");
    assert_eq!(GROUPED.digits(1_234_567, &pattern(1, 3, 2)), "12.34.567");
    assert_eq!(GROUPED.digits(7, &pattern(3, 0, 0)), "007");
    assert_eq!(GROUPED.digits(0, &pattern(1, 3, 3)), "0");
    let mut point = pattern(1, 3, 3);
    point.point = true;
    assert_eq!(GROUPED.digits(1_000, &point), "1.000,");
    point.fraction = 2;
    assert_eq!(GROUPED.digits(5, &point), "5,00");
    let arabic = Words {
        digits: "\u{660}\u{661}\u{662}\u{663}\u{664}\u{665}\u{666}\u{667}\u{668}\u{669}",
        ..GROUPED
    };
    assert_eq!(
        arabic.digits(1_203, &pattern(1, 3, 3)),
        String::from("\u{661}.\u{662}\u{660}\u{663}")
    );
}

#[test]
fn a_relation_holds_on_the_number_or_its_remainder() {
    let tens = Relation {
        equal: true,
        modulus: Some(10),
        ranges: &[(2, 4)],
    };
    assert!(tens.holds(23));
    assert!(!tens.holds(25));
    let not_eleven = Relation {
        equal: false,
        modulus: None,
        ranges: &[(11, 11)],
    };
    assert!(not_eleven.holds(1));
    assert!(!not_eleven.holds(11));
}

#[test]
fn english_ordinal_suffixes_follow_its_plural_categories() {
    let english = WORDS.get(locale("en").unwrap().words.unwrap()).unwrap();
    let categories: Vec<Category> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 101, 111]
        .into_iter()
        .map(|value| english.category(PluralKind::Ordinal, value))
        .collect();
    assert_eq!(
        categories,
        [
            Category::One,
            Category::Two,
            Category::Few,
            Category::Other,
            Category::Other,
            Category::Other,
            Category::Other,
            Category::One,
            Category::Two,
            Category::Few,
            Category::One,
            Category::Other
        ]
    );
}

#[test]
fn each_format_spelled_to_a_limit_has_a_rule_set() {
    let formats: Vec<F> = FORMATS.iter().map(|(format, _)| *format).collect();
    assert_eq!(
        formats,
        [
            F::ChineseCountingThousand,
            F::ChineseLegalSimplified,
            F::Hebrew1,
            F::HindiCounting,
            F::IdeographLegalTraditional,
            F::JapaneseCounting,
            F::JapaneseLegal,
            F::KoreanCounting,
            F::TaiwaneseCountingThousand,
            F::ThaiCounting,
            F::VietnameseCounting,
        ]
    );
}

#[test]
fn a_counted_number_written_in_digits_or_of_another_format_is_none() {
    assert_eq!(
        counted(21, F::VietnameseCounting).as_deref(),
        Some("hai m\u{1B0}\u{1A1}i m\u{1ED1}t")
    );
    assert_eq!(counted(21, F::Decimal), None);
    assert_eq!(counted(1_000_000_000_000_000_000, F::HindiCounting), None);
    assert_eq!(counted(1_000_001, F::Hebrew1), None);
}

#[test]
fn a_remainder_in_brackets_rolls_a_rule_back() {
    assert_eq!(
        counted(5_000, F::Hebrew1).as_deref(),
        Some("\u{5D4}\u{5F3} \u{5D0}\u{5DC}\u{5E4}\u{5D9}\u{5DD}")
    );
}
