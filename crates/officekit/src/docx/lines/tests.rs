use alloc::borrow::ToOwned as _;

use officekit_markup::common_simple_types::{HexadecimalColorValue, OnOffValue};
use officekit_markup::drawing::{ColorContent, ColorScheme, RGBColorModelHexVariant};
use officekit_markup::wordprocessing::{
    BorderStyles, BorderStylesMember, BottomBorder, ColorValue, LeftBorder, LeftParagraphBorder,
    LeftParagraphBorderTblWidth, MeasurementInEighthsOfAPoint, MeasurementInPoints,
    MeasurementOrPercentageValue, ParagraphBorderAboveIdenticalParagraphs,
    ParagraphBorderAboveIdenticalParagraphsTblWidth, ParagraphBorderBelowIdenticalParagraphs,
    ParagraphBorderBelowIdenticalParagraphsTblWidth, ParagraphBorderBetweenFacingPages,
    ParagraphBorderBetweenIdenticalParagraphs, ParagraphShading, PreferredTableCellWidth,
    PreferredTableWidth, PreferredWidthAfterTableRow, PreferredWidthBeforeTableRow, RightBorder,
    RightParagraphBorder, RightParagraphBorderTblWidth, ShadingPatterns, ShadingPatternsMember,
    TableCellLeadingEdgeBorder, TableCellLeadingEdgeBorderTblWidth,
    TableCellTrailingMarginException, TableCellTrailingMarginExceptionTblWidth,
    TableIndentFromLeadingMargin, TableInsideHorizontalEdgesBorder, TableInsideVerticalEdgesBorder,
    TableRowCellSpacing, TableWidthUnits, TextBorder, ThemeColor, ThemeColorMember, TopBorder,
    TopLeftBottomRightDiagonalBorder, TopRightBottomLeftDiagonalBorder, TwoDigitHexadecimalValue,
};

use super::{
    BorderLine, BorderStyle, HexColor, Rgb, ShadingFill, Stated, TableLength, ThemeChange,
    ThemeReference, WordColor,
};
use crate::drawing::scheme::{SchemeMap, ThemeColors};

/// A border of `kind` with the attributes each `Some` states, each from its text.
macro_rules! border {
    ($kind:ident, $val:expr, $sz:expr, $color:expr) => {{
        let mut border = $kind::default();
        border.val = $val.map(|text: &str| BorderStyles::from(text.to_owned()));
        border.sz = $sz.map(|text: &str| MeasurementInEighthsOfAPoint::from(text.to_owned()));
        border.color = $color.map(|text: &str| ColorValue::from(text.to_owned()));
        border
    }};
}

/// A table measurement of `kind` with the attributes each `Some` states, each from its text.
macro_rules! measurement {
    ($kind:ident, $w:expr, $type:expr) => {{
        let mut measurement = $kind::default();
        measurement.w = $w.map(|text: &str| MeasurementOrPercentageValue::from(text.to_owned()));
        measurement.r#type = $type.map(|text: &str| TableWidthUnits::from(text.to_owned()));
        measurement
    }};
}

fn line(val: Option<&str>, sz: Option<&str>, color: Option<&str>) -> BorderLine {
    BorderLine::from(&border!(TopLeftBottomRightDiagonalBorder, val, sz, color))
}

fn width(w: Option<&str>, kind: Option<&str>) -> TableLength {
    TableLength::from(&measurement!(PreferredTableWidth, w, kind))
}

fn plain(value: HexColor) -> WordColor {
    WordColor {
        theme: Stated::Value(None),
        value: Stated::Value(value),
    }
}

fn shading(val: Option<&str>, color: Option<&str>, fill: Option<&str>) -> ParagraphShading {
    let mut shading = ParagraphShading::default();
    shading.val = val.map(|text| ShadingPatterns::from(text.to_owned()));
    shading.color = color.map(|text| ColorValue::from(text.to_owned()));
    shading.fill = fill.map(|text| ColorValue::from(text.to_owned()));
    shading
}

fn hex(text: &str) -> HexColor {
    let value = u32::from_str_radix(text, 16).unwrap();
    let byte = |shift: u32| u8::try_from((value >> shift) & 0xFF).unwrap();
    HexColor::Rgb(Rgb::new(byte(16), byte(8), byte(0)))
}

fn two_digits(text: &str) -> TwoDigitHexadecimalValue {
    TwoDigitHexadecimalValue::from(text.to_owned())
}

fn srgb(text: &str) -> ColorContent {
    let mut color = RGBColorModelHexVariant::default();
    color.val = Some(HexadecimalColorValue::from(text.to_owned()));
    ColorContent::from(color)
}

/// A theme whose `accent2` is `accent2`, under the default mapping.
fn theme_with_accent2(accent2: &str) -> ThemeColors {
    let mut scheme = ColorScheme::default();
    *scheme.accent_2.content = srgb(accent2);
    ThemeColors::new(Some(&scheme), SchemeMap::default())
}

#[test]
fn a_single_border_draws_a_line_of_its_size_and_colour() {
    let found = line(Some("single"), Some("8"), Some("FF0000"));
    assert_eq!(
        found,
        BorderLine {
            color: plain(hex("FF0000")),
            frame: Stated::Value(false),
            shadow: Stated::Value(false),
            size: Stated::Value(8),
            space: Stated::Value(0),
            style: Stated::Value(BorderStyle::Line(BorderStylesMember::Single)),
        }
    );
    assert_eq!(found.draws(), Some(true));
}

#[test]
fn a_border_with_no_style_or_no_border_style_leaves_the_style_unstated() {
    for val in [None, Some("bogus")] {
        let found = line(val, Some("8"), None);
        assert_eq!(found.style, Stated::Unstated);
        assert_eq!(found.draws(), None);
        assert_eq!(found.size, Stated::Value(8));
        assert_eq!(found.color, plain(HexColor::Automatic));
    }
}

#[test]
fn nil_and_none_draw_no_border() {
    for val in ["nil", "none"] {
        let found = line(Some(val), Some("8"), Some("000000"));
        assert_eq!(found.style, Stated::Value(BorderStyle::NoBorder));
        assert_eq!(found.draws(), Some(false));
    }
}

#[test]
fn every_line_style_is_a_line_border_and_an_art_style_an_art_border() {
    let lines = [
        "dashDotStroked",
        "dashed",
        "dashSmallGap",
        "dotDash",
        "dotDotDash",
        "dotted",
        "double",
        "doubleWave",
        "inset",
        "outset",
        "single",
        "thick",
        "thickThinLargeGap",
        "thickThinMediumGap",
        "thickThinSmallGap",
        "thinThickLargeGap",
        "thinThickMediumGap",
        "thinThickSmallGap",
        "thinThickThinLargeGap",
        "thinThickThinMediumGap",
        "thinThickThinSmallGap",
        "threeDEmboss",
        "threeDEngrave",
        "triple",
        "wave",
    ];
    let member = |name: &str| BorderStyles::from(name.to_owned()).value().unwrap();
    for name in lines {
        let found = line(Some(name), None, None).style;
        assert_eq!(
            found,
            Stated::Value(BorderStyle::Line(member(name))),
            "{name}"
        );
    }
    for name in ["apples", "basicBlackDots", "custom", "zigZagStitch"] {
        let found = line(Some(name), None, None).style;
        assert_eq!(
            found,
            Stated::Value(BorderStyle::Art(member(name))),
            "{name}"
        );
    }
}

#[test]
fn an_absent_size_is_unstated_and_an_absent_space_colour_shadow_and_frame_take_their_defaults() {
    let found = line(Some("single"), None, None);
    assert_eq!(found.size, Stated::Unstated);
    assert_eq!(found.space, Stated::Value(0));
    assert_eq!(found.color, plain(HexColor::Automatic));
    assert_eq!(found.shadow, Stated::Value(false));
    assert_eq!(found.frame, Stated::Value(false));
}

#[test]
fn an_attribute_whose_text_is_no_value_is_unstated() {
    let mut border = border!(
        ParagraphBorderAboveIdenticalParagraphs,
        Some("single"),
        Some("eight"),
        Some("red")
    );
    border.space = Some(MeasurementInPoints::from("-1".to_owned()));
    border.shadow = Some(OnOffValue::from("maybe".to_owned()));
    border.frame = Some(OnOffValue::from(true));
    let found = BorderLine::from(&border);
    assert_eq!(found.size, Stated::Unstated);
    assert_eq!(found.space, Stated::Unstated);
    assert_eq!(found.color.value, Stated::Unstated);
    assert_eq!(found.shadow, Stated::Unstated);
    assert_eq!(found.frame, Stated::Value(true));
}

#[test]
fn auto_is_the_automatic_colour() {
    let found = line(Some("single"), Some("4"), Some("auto"));
    assert_eq!(found.color, plain(HexColor::Automatic));
}

#[test]
fn every_border_element_type_reads_the_same() {
    let expected = line(Some("double"), Some("12"), Some("00FF00"));
    let stated = (Some("double"), Some("12"), Some("00FF00"));
    macro_rules! each {
        ($($kind:ident),+) => {$(
            let element = border!($kind, stated.0, stated.1, stated.2);
            assert_eq!(BorderLine::from(&element), expected, stringify!($kind));
        )+};
    }
    each!(
        BottomBorder,
        LeftBorder,
        LeftParagraphBorder,
        ParagraphBorderAboveIdenticalParagraphs,
        ParagraphBorderBelowIdenticalParagraphs,
        ParagraphBorderBetweenFacingPages,
        ParagraphBorderBetweenIdenticalParagraphs,
        RightBorder,
        RightParagraphBorder,
        TableCellLeadingEdgeBorder,
        TableCellTrailingMarginException,
        TableInsideHorizontalEdgesBorder,
        TableInsideVerticalEdgesBorder,
        TextBorder,
        TopBorder,
        TopLeftBottomRightDiagonalBorder,
        TopRightBottomLeftDiagonalBorder
    );
}

#[test]
fn clear_fills_with_the_fill_and_solid_with_the_pattern_colour() {
    let clear = ShadingFill::from(&shading(Some("clear"), Some("FF0000"), Some("C3D69B")));
    assert_eq!(clear, ShadingFill::Clear(plain(hex("C3D69B"))));
    let solid = ShadingFill::from(&shading(Some("solid"), Some("FF0000"), Some("C3D69B")));
    assert_eq!(solid, ShadingFill::Solid(plain(hex("FF0000"))));
    let automatic = ShadingFill::from(&shading(Some("solid"), None, None));
    assert_eq!(automatic, ShadingFill::Solid(plain(HexColor::Automatic)));
    let auto_fill = ShadingFill::from(&shading(Some("clear"), None, Some("auto")));
    assert_eq!(auto_fill, ShadingFill::Clear(plain(HexColor::Automatic)));
}

#[test]
fn solid_with_an_automatic_pattern_colour_is_the_automatic_pattern_colour_not_the_fill() {
    let found = ShadingFill::from(&shading(Some("solid"), Some("auto"), Some("FFFFFF")));
    assert_eq!(found, ShadingFill::Solid(plain(HexColor::Automatic)));
}

#[test]
fn clear_with_an_automatic_fill_is_the_automatic_fill_not_the_pattern_colour() {
    let found = ShadingFill::from(&shading(Some("clear"), Some("FF0000"), Some("auto")));
    assert_eq!(found, ShadingFill::Clear(plain(HexColor::Automatic)));
}

#[test]
fn clear_takes_the_fill_theme_and_solid_the_pattern_theme() {
    let mut element = shading(Some("clear"), Some("FF0000"), Some("C3D69B"));
    element.theme_color = Some(ThemeColor::from("accent6".to_owned()));
    element.theme_fill = Some(ThemeColor::from("accent3".to_owned()));
    let themed = |name: ThemeColorMember, value: &str| WordColor {
        theme: Stated::Value(Some(ThemeReference {
            change: Stated::Value(ThemeChange::Plain),
            name,
        })),
        value: Stated::Value(hex(value)),
    };
    assert_eq!(
        ShadingFill::from(&element),
        ShadingFill::Clear(themed(ThemeColorMember::Accent3, "C3D69B"))
    );
    element.val = Some(ShadingPatterns::from("solid".to_owned()));
    assert_eq!(
        ShadingFill::from(&element),
        ShadingFill::Solid(themed(ThemeColorMember::Accent6, "FF0000"))
    );
}

#[test]
fn a_percent_pattern_lays_its_colour_over_its_fill() {
    let found = ShadingFill::from(&shading(Some("pct20"), Some("000000"), Some("FFFFFF")));
    assert_eq!(
        found,
        ShadingFill::Pattern {
            color: plain(hex("000000")),
            fill: plain(hex("FFFFFF")),
            pattern: ShadingPatternsMember::Pct20,
        }
    );
}

#[test]
fn nil_is_its_own_case_and_a_missing_or_unknown_pattern_is_unstated() {
    let nil = ShadingFill::from(&shading(Some("nil"), None, Some("FFFF00")));
    assert_eq!(
        nil,
        ShadingFill::Nil {
            color: plain(HexColor::Automatic),
            fill: plain(hex("FFFF00")),
        }
    );
    for val in [None, Some("pct21")] {
        let found = ShadingFill::from(&shading(val, None, Some("FFFF00")));
        assert_eq!(found, ShadingFill::Unstated);
    }
}

#[test]
fn a_width_takes_the_unit_its_type_gives() {
    assert_eq!(
        width(Some("1440"), Some("dxa")),
        TableLength::Twentieths(1440)
    );
    assert_eq!(width(Some("1440"), None), TableLength::Twentieths(1440));
    assert_eq!(
        width(Some("5000"), Some("pct")),
        TableLength::Fiftieths(5000)
    );
    assert_eq!(width(Some("0"), Some("auto")), TableLength::Automatic);
    assert_eq!(width(Some("500"), Some("nil")), TableLength::Zero);
    assert_eq!(width(Some("50%"), Some("nil")), TableLength::Zero);
}

#[test]
fn an_absent_width_is_zero_in_its_unit() {
    assert_eq!(width(None, Some("dxa")), TableLength::Twentieths(0));
    assert_eq!(width(None, Some("pct")), TableLength::Fiftieths(0));
    assert_eq!(width(None, None), TableLength::Twentieths(0));
    assert_eq!(width(None, Some("auto")), TableLength::Automatic);
}

#[test]
fn a_percent_sign_or_a_unit_overrides_a_contradictory_type() {
    assert_eq!(
        width(Some("100%"), Some("pct")),
        TableLength::Fiftieths(5000)
    );
    assert_eq!(
        width(Some("33.3%"), Some("pct")),
        TableLength::Fiftieths(1665)
    );
    assert_eq!(
        width(Some("50%"), Some("dxa")),
        TableLength::Fiftieths(2500)
    );
    assert_eq!(
        width(Some("50%"), Some("auto")),
        TableLength::Fiftieths(2500)
    );
    assert_eq!(
        width(Some("1in"), Some("pct")),
        TableLength::Twentieths(1440)
    );
    assert_eq!(
        width(Some("2.54cm"), Some("dxa")),
        TableLength::Twentieths(1440)
    );
    assert_eq!(width(Some("1mm"), None), TableLength::Twentieths(57));
    assert_eq!(width(Some("-0.5pt"), None), TableLength::Twentieths(-10));
    assert_eq!(width(Some("1pc"), None), TableLength::Twentieths(240));
    assert_eq!(width(Some("1pi"), None), TableLength::Twentieths(240));
}

#[test]
fn a_width_or_type_that_is_no_value_is_unstated() {
    assert_eq!(width(Some("wide"), Some("dxa")), TableLength::Unstated);
    assert_eq!(width(Some("100"), Some("twips")), TableLength::Unstated);
}

#[test]
fn every_table_measurement_type_reads_the_same() {
    macro_rules! each {
        ($($kind:ident),+) => {$(
            let element = measurement!($kind, Some("720"), Some("pct"));
            assert_eq!(TableLength::from(&element), TableLength::Fiftieths(720), stringify!($kind));
        )+};
    }
    each!(
        LeftParagraphBorderTblWidth,
        ParagraphBorderAboveIdenticalParagraphsTblWidth,
        ParagraphBorderBelowIdenticalParagraphsTblWidth,
        PreferredTableCellWidth,
        PreferredTableWidth,
        PreferredWidthAfterTableRow,
        PreferredWidthBeforeTableRow,
        RightParagraphBorderTblWidth,
        TableCellLeadingEdgeBorderTblWidth,
        TableCellTrailingMarginExceptionTblWidth,
        TableIndentFromLeadingMargin,
        TableRowCellSpacing
    );
}

#[test]
fn a_theme_colour_is_given_as_written_and_resolved_with_its_tint() {
    let mut border = border!(
        ParagraphBorderAboveIdenticalParagraphs,
        Some("single"),
        Some("24"),
        Some("F2DCDB")
    );
    border.theme_color = Some(ThemeColor::from("accent2".to_owned()));
    border.theme_tint = Some(two_digits("99"));
    let found = BorderLine::from(&border);
    assert_eq!(
        found.color,
        WordColor {
            theme: Stated::Value(Some(ThemeReference {
                change: Stated::Value(ThemeChange::Tint(0x99)),
                name: ThemeColorMember::Accent2,
            })),
            value: Stated::Value(hex("F2DCDB")),
        }
    );
    // Section 17.3.4: 4F81BD under a tint of 99 is 95B3D7.
    let theme = theme_with_accent2("4F81BD");
    assert_eq!(found.color.resolve(&theme), Stated::Value(hex("95B3D7")));
}

#[test]
fn a_theme_shade_darkens_the_luminance() {
    let mut border = border!(
        LeftParagraphBorder,
        Some("single"),
        Some("4"),
        Some("943634")
    );
    border.theme_color = Some(ThemeColor::from("accent2".to_owned()));
    border.theme_shade = Some(two_digits("BF"));
    let found = BorderLine::from(&border);
    assert_eq!(
        found.color.theme,
        Stated::Value(Some(ThemeReference {
            change: Stated::Value(ThemeChange::Shade(0xBF)),
            name: ThemeColorMember::Accent2,
        }))
    );
    // Section 17.3.4's example rounds the shade to 75% and the luminance to 0.53 and writes
    // 943634; its formula with BF / 255 gives 953735.
    let theme = theme_with_accent2("C0504D");
    assert_eq!(found.color.resolve(&theme), Stated::Value(hex("953735")));
}

#[test]
fn a_border_tint_overrides_its_shade() {
    let mut border = border!(RightParagraphBorder, Some("single"), None, None);
    border.theme_color = Some(ThemeColor::from("accent2".to_owned()));
    border.theme_tint = Some(two_digits("99"));
    border.theme_shade = Some(two_digits("BF"));
    let found = BorderLine::from(&border);
    let expected = Stated::Value(Some(ThemeReference {
        change: Stated::Value(ThemeChange::Tint(0x99)),
        name: ThemeColorMember::Accent2,
    }));
    assert_eq!(found.color.theme, expected);
}

#[test]
fn a_fill_with_both_a_tint_and_a_shade_has_no_stated_change() {
    let mut found = shading(Some("clear"), None, Some("FFFFFF"));
    found.theme_fill = Some(ThemeColor::from("accent2".to_owned()));
    found.theme_fill_tint = Some(two_digits("99"));
    found.theme_fill_shade = Some(two_digits("BF"));
    let fill = WordColor {
        theme: Stated::Value(Some(ThemeReference {
            change: Stated::Unstated,
            name: ThemeColorMember::Accent2,
        })),
        value: Stated::Value(hex("FFFFFF")),
    };
    assert_eq!(ShadingFill::from(&found), ShadingFill::Clear(fill));
    assert_eq!(
        fill.resolve(&theme_with_accent2("4F81BD")),
        Stated::Unstated
    );
}

#[test]
fn a_theme_colour_supersedes_the_value_and_none_or_no_theme_does_not() {
    let mut border = border!(TopBorder, Some("single"), None, Some("FF0000"));
    border.theme_color = Some(ThemeColor::from("accent2".to_owned()));
    let themed = BorderLine::from(&border).color;
    let theme = theme_with_accent2("4F81BD");
    assert_eq!(themed.resolve(&theme), Stated::Value(hex("4F81BD")));
    assert_eq!(themed.resolve(&ThemeColors::default()), Stated::Unstated);

    border.theme_color = Some(ThemeColor::from("none".to_owned()));
    let none = BorderLine::from(&border).color;
    assert_eq!(none.theme, Stated::Value(None));
    assert_eq!(none.resolve(&theme), Stated::Value(hex("FF0000")));

    border.theme_color = Some(ThemeColor::from("accent9".to_owned()));
    let unknown = BorderLine::from(&border).color;
    assert_eq!(unknown.theme, Stated::Unstated);
    assert_eq!(unknown.resolve(&theme), Stated::Unstated);
}

#[test]
fn a_tint_with_no_theme_colour_is_not_used() {
    let mut border = border!(TextBorder, Some("single"), None, Some("FF0000"));
    border.theme_tint = Some(two_digits("99"));
    let found = BorderLine::from(&border).color;
    assert_eq!(found, plain(hex("FF0000")));
    assert_eq!(
        found.resolve(&theme_with_accent2("4F81BD")),
        Stated::Value(hex("FF0000"))
    );
}

#[test]
fn each_theme_name_goes_through_the_mapping_section_17_18_97_gives_it() {
    let mut scheme = ColorScheme::default();
    *scheme.dark_1.content = srgb("000001");
    *scheme.light_1.content = srgb("FFFFF1");
    *scheme.dark_2.content = srgb("000002");
    *scheme.light_2.content = srgb("FFFFF2");
    *scheme.hyperlink.content = srgb("0000FF");
    *scheme.followed_hyperlink.content = srgb("FF00FF");
    let theme = ThemeColors::new(Some(&scheme), SchemeMap::default());
    let cases = [
        ("dark1", "000001"),
        ("text1", "000001"),
        ("light1", "FFFFF1"),
        ("background1", "FFFFF1"),
        ("dark2", "000002"),
        ("text2", "000002"),
        ("light2", "FFFFF2"),
        ("background2", "FFFFF2"),
        ("hyperlink", "0000FF"),
        ("followedHyperlink", "FF00FF"),
    ];
    for (name, expected) in cases {
        let mut border = border!(TopBorder, Some("single"), None, None);
        border.theme_color = Some(ThemeColor::from(name.to_owned()));
        let found = BorderLine::from(&border).color.resolve(&theme);
        assert_eq!(found, Stated::Value(hex(expected)), "{name}");
    }
}
