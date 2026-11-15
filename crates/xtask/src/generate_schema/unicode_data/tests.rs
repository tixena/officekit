use super::{
    CasingContext, DecimalDigit, LanguageCasing, combining_classes, decimal_digits,
    language_casings, property,
};

#[test]
fn every_character_of_category_nd_is_read_with_its_name_and_value() {
    let text = "002F;SOLIDUS;Po;0;CS;;;;;N;SLASH;;;;\n\
                0030;DIGIT ZERO;Nd;0;EN;;0;0;0;N;;;;;\n\
                00B2;SUPERSCRIPT TWO;No;0;EN;<super> 0032;;2;2;N;;;;;\n\
                FF11;FULLWIDTH DIGIT ONE;Nd;0;EN;<wide> 0031;1;1;1;N;;;;;\n";
    assert_eq!(
        decimal_digits(text).unwrap(),
        [
            DecimalDigit {
                character: '0',
                name: "DIGIT ZERO".to_owned(),
                value: 0,
            },
            DecimalDigit {
                character: '\u{FF11}',
                name: "FULLWIDTH DIGIT ONE".to_owned(),
                value: 1,
            },
        ]
    );
}

#[test]
fn a_digit_without_a_decimal_value_stops_the_reading() {
    let err = decimal_digits("0030;DIGIT ZERO;Nd;0;EN;;;0;0;N;;;;;\n").unwrap_err();
    assert!(err.to_string().contains("cannot read 0030;DIGIT ZERO"));
}

#[test]
fn the_rows_that_name_a_language_are_read_with_their_contexts() {
    let text = "# SpecialCasing-18.0.0.txt\n\
                00DF; 00DF; 0053 0073; 0053 0053; # LATIN SMALL LETTER SHARP S\n\
                03A3; 03C2; 03A3; 03A3; Final_Sigma; # GREEK CAPITAL LETTER SIGMA\n\
                00CC; 0069 0307 0300; 00CC; 00CC; lt; # LATIN CAPITAL LETTER I WITH GRAVE\n\
                0307; ; 0307; 0307; tr After_I; # COMBINING DOT ABOVE\n\
                0049; 0131; 0049; 0049; AZ not_before_dot; # LATIN CAPITAL LETTER I\n";
    assert_eq!(
        language_casings(text).unwrap(),
        [
            LanguageCasing {
                character: '\u{CC}',
                contexts: Vec::new(),
                language: "lt".to_owned(),
                lower: "i\u{307}\u{300}".to_owned(),
                upper: "\u{CC}".to_owned(),
            },
            LanguageCasing {
                character: '\u{307}',
                contexts: vec![(CasingContext::AfterI, true)],
                language: "tr".to_owned(),
                lower: String::new(),
                upper: "\u{307}".to_owned(),
            },
            LanguageCasing {
                character: 'I',
                contexts: vec![(CasingContext::BeforeDot, false)],
                language: "az".to_owned(),
                lower: "\u{131}".to_owned(),
                upper: "I".to_owned(),
            },
        ]
    );
}

#[test]
fn a_language_row_with_a_context_the_table_cannot_evaluate_stops_the_reading() {
    let err = language_casings("03A3; 03C2; 03A3; 03A3; el Final_Sigma;\n").unwrap_err();
    assert_eq!(
        format!("{err:#}"),
        "cannot read 03A3; 03C2; 03A3; 03A3; el Final_Sigma;: the condition Final_Sigma, \
         which the table cannot evaluate"
    );
}

#[test]
fn a_row_naming_two_languages_stops_the_reading() {
    let err = language_casings("0049; 0131; 0049; 0049; tr az;\n").unwrap_err();
    assert_eq!(
        format!("{err:#}"),
        "cannot read 0049; 0131; 0049; 0049; tr az;: two languages"
    );
}

#[test]
fn the_combining_classes_are_ranges_of_one_class() {
    let text = "0041;LATIN CAPITAL LETTER A;Lu;0;L;;;;;N;;;;0061;\n\
                0300;COMBINING GRAVE ACCENT;Mn;230;NSM;;;;;N;;;;;\n\
                0301;COMBINING ACUTE ACCENT;Mn;230;NSM;;;;;N;;;;;\n\
                0315;COMBINING COMMA ABOVE RIGHT;Mn;232;NSM;;;;;N;;;;;\n\
                0316;COMBINING GRAVE ACCENT BELOW;Mn;220;NSM;;;;;N;;;;;\n\
                0317;COMBINING ACUTE ACCENT BELOW;Mn;220;NSM;;;;;N;;;;;\n\
                0319;COMBINING RIGHT TACK BELOW;Mn;220;NSM;;;;;N;;;;;\n\
                3400;<CJK Ideograph Extension A, First>;Lo;0;L;;;;;N;;;;;\n\
                4DBF;<CJK Ideograph Extension A, Last>;Lo;0;L;;;;;N;;;;;\n";
    assert_eq!(
        combining_classes(text).unwrap(),
        [
            ('\u{300}', '\u{301}', 230),
            ('\u{315}', '\u{315}', 232),
            ('\u{316}', '\u{317}', 220),
            ('\u{319}', '\u{319}', 220),
        ]
    );
}

#[test]
fn a_range_of_a_combining_class_spans_its_first_and_last_lines() {
    let text = "E000;<Test, First>;Mn;7;NSM;;;;;N;;;;;\n\
                E002;<Test, Last>;Mn;7;NSM;;;;;N;;;;;\n";
    assert_eq!(
        combining_classes(text).unwrap(),
        [('\u{E000}', '\u{E002}', 7)]
    );
}

#[test]
fn a_property_s_ranges_are_read_sorted() {
    let text = "# PropList-18.0.0.txt\n\
                012F          ; Soft_Dotted # L&       LATIN SMALL LETTER I WITH OGONEK\n\
                0069..006A    ; Soft_Dotted # L&   [2] LATIN SMALL LETTER I..LATIN SMALL LETTER J\n\
                0009..000D    ; White_Space # Cc   [5] <control-0009>..<control-000D>\n";
    assert_eq!(
        property(text, "Soft_Dotted").unwrap(),
        [('i', 'j'), ('\u{12F}', '\u{12F}')]
    );
}
