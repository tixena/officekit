use alloc::string::String;
use alloc::vec::Vec;

use officekit_markup::drawing::{Font, Theme};
use officekit_markup::wordprocessing::{RunFonts, ThemeFont, ThemeFontLanguages, ThemeFontMember};

use super::{RunFontFamilies, ThemeFonts};

/// Every member of `ST_Theme`.
const MEMBERS: [ThemeFontMember; 8] = [
    ThemeFontMember::MajorAscii,
    ThemeFontMember::MajorBidi,
    ThemeFontMember::MajorEastAsia,
    ThemeFontMember::MajorHAnsi,
    ThemeFontMember::MinorAscii,
    ThemeFontMember::MinorBidi,
    ThemeFontMember::MinorEastAsia,
    ThemeFontMember::MinorHAnsi,
];

/// A theme whose minor `a:latin` is Calibri and major `a:latin` Calibri Light.
fn office() -> Theme {
    let mut theme = Theme::default();
    theme.elements.font_scheme.minor_fonts.latin_font.typeface = Some("Calibri".to_owned());
    theme.elements.font_scheme.major_font.latin_font.typeface = Some("Calibri Light".to_owned());
    theme
}

fn font(script: &str, typeface: &str) -> Font {
    let mut found = Font::default();
    found.script = Some(script.to_owned());
    found.typeface = Some(typeface.to_owned());
    found
}

fn east_asian_language(tag: &str) -> ThemeFontLanguages {
    let mut languages = ThemeFontLanguages::default();
    languages.east_asia = Some(tag.to_owned());
    languages
}

#[test]
fn a_theme_attribute_alone_gives_the_theme_s_family() {
    let fonts = ThemeFonts::new(Some(&office()), None);
    let mut run = RunFonts::default();
    run.ascii_theme = Some(ThemeFont::from(ThemeFontMember::MinorHAnsi));
    assert_eq!(fonts.families(Some(&run)).ascii.as_deref(), Some("Calibri"));
}

#[test]
fn the_theme_attribute_wins_over_the_plain_one_of_its_kind() {
    let fonts = ThemeFonts::new(Some(&office()), None);
    assert_eq!(fonts.family(ThemeFontMember::MinorHAnsi), Some("Calibri"));
    assert_eq!(
        fonts.family(ThemeFontMember::MajorAscii),
        Some("Calibri Light")
    );

    let mut run = RunFonts::default();
    run.ascii = Some("Arial".to_owned());
    run.ascii_theme = Some(ThemeFont::from(ThemeFontMember::MinorHAnsi));
    run.h_ansi = Some("Arial".to_owned());
    let families = fonts.families(Some(&run));
    assert_eq!(families.ascii.as_deref(), Some("Calibri"));
    assert_eq!(families.h_ansi.as_deref(), Some("Arial"));
    assert_eq!(families.east_asia, None);
    assert_eq!(families.cs, None);
}

#[test]
fn each_value_takes_its_region_of_its_collection() {
    let mut theme = Theme::default();
    let scheme = &mut theme.elements.font_scheme;
    scheme.major_font.latin_font.typeface = Some("Major Latin".to_owned());
    scheme.major_font.east_asian_font.typeface = Some("Major East Asian".to_owned());
    scheme.major_font.complex_script_font.typeface = Some("Major Complex".to_owned());
    scheme.minor_fonts.latin_font.typeface = Some("Minor Latin".to_owned());
    scheme.minor_fonts.east_asian_font.typeface = Some("Minor East Asian".to_owned());
    scheme.minor_fonts.complex_script_font.typeface = Some("Minor Complex".to_owned());
    let fonts = ThemeFonts::new(Some(&theme), None);

    let families: Vec<Option<&str>> = MEMBERS.iter().map(|member| fonts.family(*member)).collect();
    assert_eq!(
        families,
        [
            Some("Major Latin"),
            Some("Major Complex"),
            Some("Major East Asian"),
            Some("Major Latin"),
            Some("Minor Latin"),
            Some("Minor Complex"),
            Some("Minor East Asian"),
            Some("Minor Latin"),
        ]
    );
}

#[test]
fn a_language_with_a_script_subtag_takes_the_font_of_that_script() {
    let mut theme = Theme::default();
    let minor = &mut theme.elements.font_scheme.minor_fonts;
    minor.east_asian_font.typeface = Some(String::new());
    minor
        .font
        .extend([font("Hans", "SimSun"), font("Hant", "PMingLiU")]);
    let languages = east_asian_language("zh-Hant-TW");
    let fonts = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(
        fonts.family(ThemeFontMember::MinorEastAsia),
        Some("PMingLiU")
    );
    assert_eq!(fonts.family(ThemeFontMember::MajorEastAsia), None);

    let lower = east_asian_language("zh-hant-TW");
    let lower_fonts = ThemeFonts::new(Some(&theme), Some(&lower));
    assert_eq!(
        lower_fonts.family(ThemeFontMember::MinorEastAsia),
        Some("PMingLiU")
    );
}

#[test]
fn a_language_with_no_script_subtag_falls_back_to_the_region_s_typeface() {
    let mut theme = Theme::default();
    let minor = &mut theme.elements.font_scheme.minor_fonts;
    minor.east_asian_font.typeface = Some(String::new());
    minor.font.push(font("Jpan", "MS Mincho"));
    let languages = east_asian_language("ja-JP");
    let fonts = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(fonts.family(ThemeFontMember::MinorEastAsia), None);

    theme
        .elements
        .font_scheme
        .minor_fonts
        .east_asian_font
        .typeface = Some("Yu Mincho".to_owned());
    let named = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(
        named.family(ThemeFontMember::MinorEastAsia),
        Some("Yu Mincho")
    );
}

#[test]
fn a_font_whose_script_is_the_language_itself_is_the_language_s() {
    let mut theme = Theme::default();
    theme
        .elements
        .font_scheme
        .major_font
        .font
        .push(font("ja-JP", "MS Mincho"));
    let languages = east_asian_language("ja-JP");
    let fonts = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(
        fonts.family(ThemeFontMember::MajorEastAsia),
        Some("MS Mincho")
    );
}

#[test]
fn each_language_attribute_picks_for_its_own_values() {
    let mut theme = office();
    let major = &mut theme.elements.font_scheme.major_font;
    major.east_asian_font.typeface = Some("Major East Asian".to_owned());
    major.complex_script_font.typeface = Some("Major Complex".to_owned());
    major
        .font
        .extend([font("Cyrl", "Cyrillic Font"), font("Arab", "Arabic Font")]);
    let mut languages = ThemeFontLanguages::default();
    languages.val = Some("sr-Cyrl-RS".to_owned());
    languages.bidi = Some("ar-Arab-SA".to_owned());
    let fonts = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(
        fonts.family(ThemeFontMember::MajorAscii),
        Some("Cyrillic Font")
    );
    assert_eq!(
        fonts.family(ThemeFontMember::MajorHAnsi),
        Some("Cyrillic Font")
    );
    assert_eq!(
        fonts.family(ThemeFontMember::MajorBidi),
        Some("Arabic Font")
    );
    assert_eq!(
        fonts.family(ThemeFontMember::MajorEastAsia),
        Some("Major East Asian")
    );
    assert_eq!(fonts.family(ThemeFontMember::MinorAscii), Some("Calibri"));
}

#[test]
fn an_empty_typeface_is_no_family() {
    let mut theme = Theme::default();
    theme
        .elements
        .font_scheme
        .major_font
        .east_asian_font
        .typeface = Some(String::new());
    let fonts = ThemeFonts::new(Some(&theme), None);
    assert_eq!(fonts.family(ThemeFontMember::MajorEastAsia), None);
}

#[test]
fn with_no_theme_a_theme_attribute_names_no_family() {
    let fonts = ThemeFonts::new(None, None);
    for member in MEMBERS {
        assert_eq!(fonts.family(member), None);
    }
    let mut run = RunFonts::default();
    run.ascii = Some("Arial".to_owned());
    run.h_ansi_theme = Some(ThemeFont::from(ThemeFontMember::MinorHAnsi));
    let families = fonts.families(Some(&run));
    assert_eq!(families.ascii.as_deref(), Some("Arial"));
    assert_eq!(families.h_ansi, None);
}

#[test]
fn a_theme_attribute_that_is_no_value_leaves_the_plain_one() {
    let fonts = ThemeFonts::new(Some(&office()), None);
    let mut run = RunFonts::default();
    run.ascii = Some("Arial".to_owned());
    run.ascii_theme = Some(ThemeFont::from("nonsense".to_owned()));
    assert_eq!(fonts.families(Some(&run)).ascii.as_deref(), Some("Arial"));
}

#[test]
fn no_fonts_give_no_family() {
    let fonts = ThemeFonts::new(Some(&office()), None);
    assert_eq!(fonts.families(None), RunFontFamilies::default());
}
