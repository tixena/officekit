use officekit_markup::common_simple_types::HexadecimalColorValue;
use officekit_markup::drawing::{
    Alpha, AlphaModulation, AlphaOffset, Angle, Blue, BlueModulation, BlueOffset, ColorContent,
    ColorScheme, ColorTransformContent, Complement, FixedPercentage, Gamma, Gray, Green,
    GreenModulation, GreenOffset, Hue, HueModulate, HueOffset, HueSaturationLuminanceColorModel,
    Inverse, InverseGamma, Luminance, LuminanceModulation, LuminanceOffset, Percentage,
    PositiveFixedAngle, PositiveFixedPercentage, PositivePercentageValueWithSign, PresetColor,
    PresetColorValue, PresetColorValueMember, RGBColorModelHexVariant,
    RGBColorModelPercentageVariant, Red, RedModulation, RedOffset, Saturation,
    SaturationModulation, SaturationOffset, SchemeColor, Shade, StSchemeColorVal,
    StSchemeColorValMember, SystemColor, SystemColorValue, Tint,
};
use officekit_xml::element::Node;

use super::preset::PRESET_COLORS;
use super::{PercentKind, Rgba, SCALE, color, encoded, linear};
use crate::drawing::scheme::{SchemeMap, ThemeColors};

/// The opaque colour of six hexadecimal digits.
fn rgba(digits: &str) -> Rgba {
    let byte = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).unwrap();
    Rgba::new(byte(0), byte(2), byte(4), 100_000)
}

/// An `a:srgbClr` of `digits` with `changes`.
fn hex(digits: &str, changes: Vec<ColorTransformContent>) -> ColorContent {
    let mut color = RGBColorModelHexVariant::default();
    color.val = Some(HexadecimalColorValue::from(digits.to_owned()));
    color.content = changes;
    ColorContent::from(color)
}

/// The colour of `content` with no theme and no placeholder.
fn plain(content: &ColorContent) -> Option<Rgba> {
    color(content, &ThemeColors::default(), None)
}

/// The colour of `digits` after `change`.
fn changed(digits: &str, change: ColorTransformContent) -> Option<Rgba> {
    plain(&hex(digits, vec![change]))
}

/// `text` as an `ST_Percentage`.
fn percent(text: &str) -> Percentage {
    Percentage::from(text.to_owned())
}

/// `text` as an `ST_PositiveFixedPercentage`.
fn fixed(text: &str) -> PositiveFixedPercentage {
    PositiveFixedPercentage::from(text.to_owned())
}

/// `text` as an `ST_PositivePercentage`.
fn positive(text: &str) -> PositivePercentageValueWithSign {
    PositivePercentageValueWithSign::from(text.to_owned())
}

fn shade(text: &str) -> ColorTransformContent {
    let mut shade = Shade::default();
    shade.val = Some(fixed(text));
    ColorTransformContent::from(shade)
}

fn tint(text: &str) -> ColorTransformContent {
    let mut tint = Tint::default();
    tint.val = Some(fixed(text));
    ColorTransformContent::from(tint)
}

fn alpha(text: &str) -> ColorTransformContent {
    let mut alpha = Alpha::default();
    alpha.val = Some(fixed(text));
    ColorTransformContent::from(alpha)
}

/// A scheme colour of `name` with `changes`.
fn scheme(name: StSchemeColorValMember, changes: Vec<ColorTransformContent>) -> ColorContent {
    let mut scheme = SchemeColor::default();
    scheme.val = Some(StSchemeColorVal::from(name));
    scheme.content = changes;
    ColorContent::from(scheme)
}

/// An `a:hslClr` of `hue` in 60000ths of a degree, `sat` and `lum`, with `changes`.
fn hsl(hue: &str, sat: &str, lum: &str, changes: Vec<ColorTransformContent>) -> ColorContent {
    let mut color = HueSaturationLuminanceColorModel::default();
    color.hue = Some(PositiveFixedAngle::from(hue.to_owned()));
    color.sat = Some(percent(sat));
    color.lum = Some(percent(lum));
    color.content = changes;
    ColorContent::from(color)
}

#[test]
fn the_standards_shade_written_either_way() {
    for half in ["50%", "50000"] {
        assert_eq!(changed("00FF00", shade(half)), Some(rgba("00BC00")));
    }
}

#[test]
fn the_standards_tint() {
    assert_eq!(changed("00FF00", tint("50%")), Some(rgba("BCFFBC")));
}

#[test]
fn a_shade_of_an_accent() {
    assert_eq!(changed("4472C4", shade("50%")), Some(rgba("2F528F")));
}

#[test]
fn a_scheme_colour_nothing_gives_is_no_colour() {
    let accent = scheme(StSchemeColorValMember::Accent1, Vec::new());
    assert_eq!(plain(&accent), None);
    let placeholder = scheme(StSchemeColorValMember::PhClr, Vec::new());
    assert_eq!(plain(&placeholder), None);
    let mut unnamed = SchemeColor::default();
    unnamed.val = Some(StSchemeColorVal::from("accent9".to_owned()));
    assert_eq!(plain(&ColorContent::from(unnamed)), None);
}

#[test]
fn a_scheme_colour_is_the_themes_with_its_own_changes() {
    let mut colors = ColorScheme::default();
    *colors.accent_1.content = hex("4472C4", Vec::new());
    let theme = ThemeColors::new(Some(&colors), SchemeMap::default());
    let mut darker = LuminanceModulation::default();
    darker.val = Some(percent("75000"));
    let accent = scheme(
        StSchemeColorValMember::Accent1,
        vec![ColorTransformContent::from(darker)],
    );
    assert_eq!(color(&accent, &theme, None), Some(rgba("2F5597")));
}

#[test]
fn a_theme_colour_that_names_a_scheme_colour_is_no_colour() {
    let mut colors = ColorScheme::default();
    *colors.accent_1.content = scheme(StSchemeColorValMember::Accent1, Vec::new());
    let theme = ThemeColors::new(Some(&colors), SchemeMap::default());
    let accent = scheme(StSchemeColorValMember::Accent1, Vec::new());
    assert_eq!(color(&accent, &theme, None), None);
}

#[test]
fn ph_clr_is_the_placeholder_with_its_own_changes() {
    let held = Some(rgba("4472C4"));
    let shaded = scheme(StSchemeColorValMember::PhClr, vec![shade("50%")]);
    assert_eq!(
        color(&shaded, &ThemeColors::default(), held),
        Some(rgba("2F528F"))
    );
    let bare = scheme(StSchemeColorValMember::PhClr, Vec::new());
    assert_eq!(color(&bare, &ThemeColors::default(), held), held);
}

#[test]
fn the_standards_scrgb_colour() {
    let mut color = RGBColorModelPercentageVariant::default();
    color.r = Some(percent("50%"));
    color.g = Some(percent("50%"));
    color.b = Some(percent("50%"));
    assert_eq!(plain(&ColorContent::from(color)), Some(rgba("BCBCBC")));
}

#[test]
fn a_system_colour_is_its_last_colour() {
    let mut window_text = SystemColor::default();
    window_text.val = Some(SystemColorValue::from("windowText".to_owned()));
    window_text.last_clr = Some(HexadecimalColorValue::from("000000".to_owned()));
    let mut never_computed = window_text.clone();
    never_computed.last_clr = None;
    assert_eq!(
        plain(&ColorContent::from(window_text)),
        Some(rgba("000000"))
    );
    assert_eq!(plain(&ColorContent::from(never_computed)), None);
}

#[test]
fn every_preset_has_the_colour_the_standard_states() {
    assert_eq!(PRESET_COLORS.len(), 190);
    for &(name, [red, green, blue]) in &PRESET_COLORS {
        let value = PresetColorValue::from(name.to_owned());
        assert!(value.value().is_ok(), "{name}");
        let mut preset = PresetColor::default();
        preset.val = Some(value);
        assert_eq!(
            plain(&ColorContent::from(preset)),
            Some(Rgba::new(red, green, blue, 100_000)),
            "{name}"
        );
    }
    for (member, digits) in [
        (PresetColorValueMember::AliceBlue, "F0F8FF"),
        (PresetColorValueMember::Black, "000000"),
    ] {
        let mut preset = PresetColor::default();
        preset.val = Some(PresetColorValue::from(member));
        assert_eq!(plain(&ColorContent::from(preset)), Some(rgba(digits)));
    }
}

#[test]
fn a_colour_whose_value_is_no_value_is_no_colour() {
    let mut unknown = PresetColor::default();
    unknown.val = Some(PresetColorValue::from("chartreuseish".to_owned()));
    assert_eq!(plain(&ColorContent::from(unknown)), None);
    assert_eq!(plain(&ColorContent::from(PresetColor::default())), None);
    assert_eq!(plain(&hex("00FF0", Vec::new())), None);
    let absent = RGBColorModelHexVariant::default();
    assert_eq!(plain(&ColorContent::from(absent)), None);
    let other = ColorContent::Other(Box::new(Node::Comment("a colour".to_owned())));
    assert_eq!(plain(&other), None);
}

#[test]
fn the_opacity_is_set_multiplied_and_shifted() {
    let opacity = |found: Option<Rgba>| found.map(|rgba| rgba.opacity);
    assert_eq!(opacity(changed("00FF00", alpha("50%"))), Some(50_000));
    let mut modulation = AlphaModulation::default();
    modulation.val = Some(positive("50%"));
    let quarter = hex(
        "00FF00",
        vec![alpha("50%"), ColorTransformContent::from(modulation)],
    );
    assert_eq!(opacity(plain(&quarter)), Some(25_000));
    let mut offset = AlphaOffset::default();
    offset.val = Some(FixedPercentage::from("10%".to_owned()));
    let full = changed("00FF00", ColorTransformContent::from(offset));
    assert_eq!(opacity(full), Some(100_000));
}

#[test]
fn the_standards_examples_of_red() {
    let mut red = Red::default();
    red.val = Some(percent("100%"));
    let set = changed("00FF00", ColorTransformContent::from(red));
    assert_eq!(set, Some(rgba("FFFF00")));
    let mut modulation = RedModulation::default();
    modulation.val = Some(percent("50%"));
    let multiplied = changed("FF0000", ColorTransformContent::from(modulation));
    assert_eq!(multiplied, Some(rgba("800000")));
    let mut offset = RedOffset::default();
    offset.val = Some(percent("-20%"));
    let shifted = changed("FF0000", ColorTransformContent::from(offset));
    assert_eq!(shifted, Some(rgba("CC0000")));
}

#[test]
fn the_standards_examples_of_green() {
    let mut green = Green::default();
    green.val = Some(percent("100%"));
    let set = changed("0000FF", ColorTransformContent::from(green));
    assert_eq!(set, Some(rgba("00FFFF")));
    let mut modulation = GreenModulation::default();
    modulation.val = Some(percent("50%"));
    let multiplied = changed("00FF00", ColorTransformContent::from(modulation));
    assert_eq!(multiplied, Some(rgba("008000")));
    let mut offset = GreenOffset::default();
    offset.val = Some(percent("-20%"));
    let shifted = changed("00FF00", ColorTransformContent::from(offset));
    assert_eq!(shifted, Some(rgba("00CC00")));
}

#[test]
fn the_standards_examples_of_blue() {
    let mut blue = Blue::default();
    blue.val = Some(percent("100%"));
    let set = changed("00FF00", ColorTransformContent::from(blue));
    assert_eq!(set, Some(rgba("00FFFF")));
    let mut modulation = BlueModulation::default();
    modulation.val = Some(percent("50%"));
    let multiplied = changed("0000FF", ColorTransformContent::from(modulation));
    assert_eq!(multiplied, Some(rgba("000080")));
    let mut offset = BlueOffset::default();
    offset.val = Some(percent("-20%"));
    let shifted = changed("0000FF", ColorTransformContent::from(offset));
    assert_eq!(shifted, Some(rgba("0000CC")));
}

#[test]
fn the_standards_examples_of_hue_saturation_and_luminance() {
    let mut lum = Luminance::default();
    lum.val = Some(percent("20%"));
    let set = changed("00FF00", ColorTransformContent::from(lum));
    assert_eq!(set, Some(rgba("006600")));
    let mut lum_offset = LuminanceOffset::default();
    lum_offset.val = Some(percent("-20%"));
    let darker = changed("00FF00", ColorTransformContent::from(lum_offset));
    assert_eq!(darker, Some(rgba("009900")));
    let mut sat_modulation = SaturationModulation::default();
    sat_modulation.val = Some(percent("20%"));
    let duller = changed("00FF00", ColorTransformContent::from(sat_modulation));
    assert_eq!(duller, Some(rgba("669966")));
    let mut hue_modulation = HueModulate::default();
    hue_modulation.val = Some(positive("50%"));
    let yellow = changed("00FF00", ColorTransformContent::from(hue_modulation));
    assert_eq!(yellow, Some(rgba("FFFF00")));
    let mut hue = Hue::default();
    hue.val = Some(PositiveFixedAngle::from("14400000".to_owned()));
    let blue = hsl("0", "100%", "50%", vec![ColorTransformContent::from(hue)]);
    assert_eq!(
        plain(&blue),
        plain(&hsl("14400000", "100%", "50%", Vec::new()))
    );
    let mut hue_offset = HueOffset::default();
    hue_offset.val = Some(Angle::from("600000".to_owned()));
    let turned = hsl(
        "0",
        "100%",
        "50%",
        vec![ColorTransformContent::from(hue_offset)],
    );
    assert_eq!(plain(&turned), Some(rgba("FF2B00")));
}

/// The standard's examples write 40C040 and 19E519, which change 00FF00's luminance; these keep it.
#[test]
fn a_saturation_set_and_shifted_keeps_the_hue_and_the_luminance() {
    let mut sat = Saturation::default();
    sat.val = Some(percent("50%"));
    let set = changed("00FF00", ColorTransformContent::from(sat));
    assert_eq!(set, Some(rgba("40BF40")));
    let mut sat_offset = SaturationOffset::default();
    sat_offset.val = Some(percent("-20%"));
    let greyer = changed("00FF00", ColorTransformContent::from(sat_offset));
    assert_eq!(greyer, Some(rgba("1AE61A")));
}

#[test]
fn a_luminance_modulation_of_half_halves_the_luminance() {
    let mut lum_modulation = LuminanceModulation::default();
    lum_modulation.val = Some(percent("50%"));
    let darker = changed("00FF00", ColorTransformContent::from(lum_modulation));
    assert_eq!(darker, Some(rgba("008000")));
}

#[test]
fn the_complement_and_the_inverse_of_red_are_cyan() {
    let complement = ColorTransformContent::from(Complement::default());
    assert_eq!(changed("FF0000", complement), Some(rgba("00FFFF")));
    let inverse = ColorTransformContent::from(Inverse::default());
    assert_eq!(changed("FF0000", inverse), Some(rgba("00FFFF")));
}

#[test]
fn the_changes_apply_in_the_files_order() {
    let tint_first = plain(&hex("00FF00", vec![tint("50%"), shade("50%")]));
    let shade_first = plain(&hex("00FF00", vec![shade("50%"), tint("50%")]));
    assert_eq!(tint_first, Some(rgba("89BC89")));
    assert_eq!(shade_first, Some(rgba("BCE1BC")));
}

#[test]
fn a_change_whose_value_is_no_number_leaves_the_colour() {
    let unchanged = Some(rgba("4472C4"));
    for text in ["nonsense", "101%", "-5%", "100001", "50.125%"] {
        assert_eq!(changed("4472C4", shade(text)), unchanged, "{text}");
    }
    let absent = ColorTransformContent::from(Shade::default());
    assert_eq!(changed("4472C4", absent), unchanged);
    let other = ColorTransformContent::Other(Box::new(Node::Comment("a change".to_owned())));
    assert_eq!(changed("4472C4", other), unchanged);
}

#[test]
fn a_hsl_colour() {
    assert_eq!(
        plain(&hsl("0", "100%", "50%", Vec::new())),
        Some(rgba("FF0000"))
    );
    assert_eq!(
        plain(&hsl("14400000", "100%", "50%", Vec::new())),
        Some(rgba("0000FF"))
    );
}

#[test]
fn gamma_then_inverse_gamma_gives_the_colour_back() {
    let both = hex(
        "808080",
        vec![
            ColorTransformContent::from(Gamma::default()),
            ColorTransformContent::from(InverseGamma::default()),
        ],
    );
    assert_eq!(plain(&both), Some(rgba("808080")));
}

#[test]
fn gray_gives_three_equal_components() {
    let gray = changed("FF0000", ColorTransformContent::from(Gray::default()));
    assert_eq!(gray, Some(rgba("7F7F7F")));
}

#[test]
fn the_srgb_curve_at_its_ends_and_at_the_ends_of_its_linear_part() {
    assert_eq!(linear(0), 0);
    assert_eq!(linear(SCALE), SCALE);
    let knee = 4045 * SCALE / 100_000;
    assert_eq!(linear(knee), 79_836);
    assert_eq!(linear(knee + 1), 79_836);
    assert_eq!(encoded(0), 0);
    assert_eq!(encoded(SCALE), SCALE);
    let mut previous = 0;
    for step in 0..=1000 {
        let value = linear(SCALE * step / 1000);
        assert!(value >= previous, "{step}");
        previous = value;
    }
    for step in knee - 100..=knee + 100 {
        assert!(linear(step) <= linear(step + 1), "{step}");
    }
}

#[test]
fn a_percentage_is_read_from_its_text() {
    assert_eq!(PercentKind::Any.thousandths("50000"), Some(50_000));
    assert_eq!(PercentKind::Any.thousandths("50%"), Some(50_000));
    assert_eq!(PercentKind::Any.thousandths("-33.3333%"), Some(-33_333));
    assert_eq!(PercentKind::Any.thousandths("12.0005%"), Some(12_001));
    assert_eq!(PercentKind::Any.thousandths("250%"), Some(250_000));
    assert_eq!(PercentKind::Any.thousandths("5.%"), None);
    assert_eq!(PercentKind::Any.thousandths("%"), None);
    assert_eq!(PercentKind::Positive.thousandths("-1%"), None);
    assert_eq!(PercentKind::Fixed.thousandths("-100%"), Some(-100_000));
    assert_eq!(PercentKind::Fixed.thousandths("100.5%"), Some(100_500));
    assert_eq!(PercentKind::Fixed.thousandths("101%"), None);
    assert_eq!(PercentKind::Fixed.thousandths("10.125%"), None);
    assert_eq!(PercentKind::Fixed.thousandths("-100001"), None);
    assert_eq!(PercentKind::PositiveFixed.thousandths("-10%"), None);
    assert_eq!(
        PercentKind::PositiveFixed.thousandths("100000"),
        Some(100_000)
    );
}
