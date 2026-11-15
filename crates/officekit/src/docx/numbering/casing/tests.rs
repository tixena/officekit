use super::{
    CASE_BY_LANGUAGE, COMBINING_CLASSES, Context, SOFT_DOTTED, capitalized, lowered, tailored,
};

#[test]
fn the_tables_are_sorted_for_their_searches() {
    assert!(CASE_BY_LANGUAGE.is_sorted_by(
        |left, right| (left.language, left.character) <= (right.language, right.character)
    ));
    assert!(
        COMBINING_CLASSES
            .windows(2)
            .all(|pair| matches!(pair, [left, right] if left.1 < right.0))
    );
    assert!(SOFT_DOTTED.is_sorted());
}

#[test]
fn turkish_and_azerbaijani_dot_the_capital_of_i_and_not_the_small_of_i() {
    for language in ["tr", "tr-TR", "AZ-latn-AZ", "az_AZ"] {
        assert_eq!(capitalized("iki", language), "\u{130}ki", "{language}");
        assert_eq!(lowered("I", language), "\u{131}", "{language}");
        assert_eq!(lowered("\u{130}", language), "i", "{language}");
        assert_eq!(lowered("I\u{307}", language), "i", "{language}");
        assert_eq!(
            lowered("I\u{316}\u{307}", language),
            "i\u{316}",
            "{language}"
        );
        assert_eq!(
            lowered("I\u{300}\u{307}", language),
            "\u{131}\u{300}\u{307}",
            "{language}"
        );
    }
}

#[test]
fn lithuanian_keeps_the_dot_of_i_under_an_accent() {
    assert_eq!(lowered("I\u{300}", "lt-LT"), "i\u{307}\u{300}");
    assert_eq!(lowered("J\u{301}", "lt"), "j\u{307}\u{301}");
    assert_eq!(
        lowered("\u{12E}\u{316}\u{303}", "lt"),
        "\u{12F}\u{307}\u{316}\u{303}"
    );
    assert_eq!(
        lowered("\u{CC}\u{CD}\u{128}", "lt"),
        "i\u{307}\u{300}i\u{307}\u{301}i\u{307}\u{303}"
    );
    assert_eq!(lowered("I\u{12E}Y", "lt"), "i\u{12F}y");
    assert_eq!(capitalized("\u{307}x", "lt"), "\u{307}x");
}

#[test]
fn each_context_skips_the_marks_of_other_classes_and_stops_at_class_0_or_230() {
    assert!(Context::AfterSoftDotted.holds("i\u{316}", ""));
    assert!(Context::AfterSoftDotted.holds("\u{1D422}", ""));
    assert!(!Context::AfterSoftDotted.holds("i\u{300}", ""));
    assert!(!Context::AfterSoftDotted.holds("I", ""));
    assert!(!Context::AfterSoftDotted.holds("", "i"));
    assert!(Context::AfterI.holds("xI\u{316}", ""));
    assert!(!Context::AfterI.holds("I\u{300}", ""));
    assert!(Context::BeforeDot.holds("", "\u{316}\u{307}"));
    assert!(!Context::BeforeDot.holds("", "\u{300}\u{307}"));
    assert!(!Context::BeforeDot.holds("", "a\u{307}"));
    assert!(Context::MoreAbove.holds("", "\u{316}\u{301}"));
    assert!(!Context::MoreAbove.holds("", "a\u{301}"));
    assert!(!Context::MoreAbove.holds("", ""));
}

#[test]
fn a_lithuanian_dot_above_after_a_soft_dotted_letter_is_kept_in_lower_case() {
    assert_eq!(lowered("i\u{307}", "lt"), "i\u{307}");
    assert_eq!(lowered("i\u{316}\u{307}", "lt"), "i\u{316}\u{307}");
}

#[test]
fn another_language_maps_by_unicode_s_defaults() {
    for language in ["en-US", "de", "", "x-none", "tt", "lv"] {
        assert!(!tailored(language), "{language}");
        assert_eq!(capitalized("iki", language), "Iki", "{language}");
        assert_eq!(lowered("I\u{130}", language), "ii\u{307}", "{language}");
    }
    for language in ["tr", "az", "lt"] {
        assert!(tailored(language), "{language}");
    }
}

#[test]
fn an_empty_text_stays_empty() {
    assert_eq!(capitalized("", "tr"), "");
    assert_eq!(lowered("", "tr"), "");
}
