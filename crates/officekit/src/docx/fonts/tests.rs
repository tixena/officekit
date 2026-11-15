use alloc::string::String;
use alloc::vec::Vec;

use officekit_markup::drawing::{Font, Theme};
use officekit_markup::wordprocessing::{RunFonts, ThemeFont, ThemeFontLanguages, ThemeFontMember};

use super::{RunFontFamilies, ThemeFonts, likely_subtags};

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

/// A theme whose major and minor collections have an empty `a:ea` and an `a:font` for each script
/// of `fonts`.
fn by_script(fonts: &[(&str, &str)]) -> Theme {
    let listed: Vec<Font> = fonts
        .iter()
        .map(|(script, typeface)| font(script, typeface))
        .collect();
    let mut theme = Theme::default();
    let scheme = &mut theme.elements.font_scheme;
    scheme.major_font.east_asian_font.typeface = Some(String::new());
    scheme.major_font.font.clone_from(&listed);
    scheme.minor_fonts.east_asian_font.typeface = Some(String::new());
    scheme.minor_fonts.font = listed;
    theme
}

#[test]
fn the_standard_s_example_maps_ja_jp_to_the_font_of_jpan() {
    let theme = by_script(&[("Jpan", "MS Mincho")]);
    let mut languages = ThemeFontLanguages::default();
    languages.val = Some("ja-JP".to_owned());
    let fonts = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(fonts.family(ThemeFontMember::MajorAscii), Some("MS Mincho"));
    assert_eq!(fonts.family(ThemeFontMember::MajorHAnsi), Some("MS Mincho"));
}

#[test]
fn an_east_asian_language_with_no_script_subtag_takes_the_font_of_its_likely_script() {
    let theme = by_script(&[("Jpan", "MS Mincho")]);
    let languages = east_asian_language("ja-JP");
    let fonts = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(
        fonts.family(ThemeFontMember::MajorEastAsia),
        Some("MS Mincho")
    );
    assert_eq!(
        fonts.family(ThemeFontMember::MinorEastAsia),
        Some("MS Mincho")
    );
}

#[test]
fn the_region_decides_the_likely_script() {
    let theme = by_script(&[("Hans", "SimSun"), ("Hant", "PMingLiU")]);
    let family = |tag: &str| {
        let languages = east_asian_language(tag);
        ThemeFonts::new(Some(&theme), Some(&languages))
            .family(ThemeFontMember::MinorEastAsia)
            .map(str::to_owned)
    };
    assert_eq!(family("zh-TW").as_deref(), Some("PMingLiU"));
    assert_eq!(family("zh-CN").as_deref(), Some("SimSun"));
    assert_eq!(family("zh-HK").as_deref(), Some("PMingLiU"));
    assert_eq!(family("zh").as_deref(), Some("SimSun"));
    assert_eq!(family("ZH-tw").as_deref(), Some("PMingLiU"));
}

#[test]
fn a_script_subtag_wins_over_the_likely_script() {
    let theme = by_script(&[("Hans", "SimSun"), ("Hant", "PMingLiU")]);
    let languages = east_asian_language("zh-Hant-CN");
    let fonts = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(
        fonts.family(ThemeFontMember::MinorEastAsia),
        Some("PMingLiU")
    );

    let private = east_asian_language("zh-CN-x-hant");
    let private_fonts = ThemeFonts::new(Some(&theme), Some(&private));
    assert_eq!(
        private_fonts.family(ThemeFontMember::MinorEastAsia),
        Some("SimSun")
    );
}

#[test]
fn a_language_with_no_likely_script_falls_back_to_the_region_s_typeface() {
    let mut theme = by_script(&[("Jpan", "MS Mincho"), ("Arab", "Arabic Font")]);
    let mut languages = ThemeFontLanguages::default();
    languages.val = Some("xx-XX".to_owned());
    languages.east_asia = Some("xx-XX".to_owned());
    languages.bidi = Some("xx-XX".to_owned());
    let fonts = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(fonts.family(ThemeFontMember::MinorEastAsia), None);

    let minor = &mut theme.elements.font_scheme.minor_fonts;
    minor.east_asian_font.typeface = Some("Yu Mincho".to_owned());
    minor.latin_font.typeface = Some("Calibri".to_owned());
    minor.complex_script_font.typeface = Some("Arial".to_owned());
    let named = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(
        named.family(ThemeFontMember::MinorEastAsia),
        Some("Yu Mincho")
    );
    assert_eq!(named.family(ThemeFontMember::MinorAscii), Some("Calibri"));
    assert_eq!(named.family(ThemeFontMember::MinorBidi), Some("Arial"));
}

#[test]
fn a_likely_script_the_theme_lacks_falls_back_to_the_region_s_typeface() {
    let mut theme = by_script(&[("Hant", "PMingLiU")]);
    theme
        .elements
        .font_scheme
        .minor_fonts
        .east_asian_font
        .typeface = Some("Yu Mincho".to_owned());
    let languages = east_asian_language("ja-JP");
    let fonts = ThemeFonts::new(Some(&theme), Some(&languages));
    assert_eq!(
        fonts.family(ThemeFontMember::MinorEastAsia),
        Some("Yu Mincho")
    );
}

#[test]
fn likely_subtags_gives_cldr_s_full_id() {
    assert_eq!(likely_subtags("zh_TW"), Some("zh_Hant_TW"));
    assert_eq!(likely_subtags("ja"), Some("ja_Jpan_JP"));
    assert_eq!(likely_subtags("und_TW"), Some("zh_Hant_TW"));
    assert_eq!(likely_subtags("xx"), None);
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
