use officekit_markup::common_simple_types::HexadecimalColorValue;
use officekit_markup::drawing::{ColorContent, ColorScheme, RGBColorModelHexVariant, Theme};
use officekit_markup::spreadsheet::{
    BackgroundColor, ColorIndexes, Colors, DataBarColor, ForegroundColor, HexUnsignedInteger,
    RGBColor, SheetTabColor, StyleSheet,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::write::WriteOptions;

use super::{Palette, Resolved, SheetColor, color, resolved, theme_slot};
use crate::drawing::color::Rgba;
use crate::drawing::scheme::{SchemeMap, ThemeColors, ThemeSlot};
use crate::parts::kinds::SpreadsheetKind;
use crate::parts::part_root::{PartRoot, store};
use crate::parts::{styles, theme};
use crate::xlsx::Xlsx;

/// Store `root` at `part` in the Transitional class and relate it from the workbook part.
fn add<R>(
    xlsx: &mut Xlsx,
    part: &str,
    content_type: &str,
    root: &R,
    relationship_type: &RelationshipType,
) where
    R: PartRoot,
{
    let name = PartName::new(part).unwrap();
    store(
        xlsx.package_mut(),
        &name,
        content_type,
        root,
        ConformanceClass::Transitional,
        WriteOptions::default(),
    )
    .unwrap();
    let main = xlsx.workbook_part().clone();
    xlsx.package_mut()
        .relate(Some(&main), &name, relationship_type.transitional)
        .unwrap();
}

fn opaque(red: u8, green: u8, blue: u8) -> Rgba {
    Rgba::new(red, green, blue, 100_000)
}

/// A colour element stating `rgb`, `theme`, `indexed` and `tint`.
fn element(
    rgb: Option<&str>,
    theme: Option<&str>,
    indexed: Option<&str>,
    tint: Option<&str>,
) -> DataBarColor {
    let mut node = DataBarColor::default();
    node.rgb = rgb.map(|value| HexUnsignedInteger::from(value.to_owned()));
    node.theme = theme.map(Box::from);
    node.indexed = indexed.map(Box::from);
    node.tint = tint.map(Box::from);
    node
}

/// The colour of `node` under `theme` and the standard's palette.
fn of(node: &DataBarColor, theme: &ThemeColors) -> Option<Rgba> {
    color(&SheetColor::from(node), theme, &Palette::default())
}

/// The colour of `rgb` with `tint`, with no theme.
fn tinted(rgb: &str, tint: &str) -> Option<Rgba> {
    of(
        &element(Some(rgb), None, None, Some(tint)),
        &ThemeColors::default(),
    )
}

fn hex(value: &str) -> ColorContent {
    let mut node = RGBColorModelHexVariant::default();
    node.val = Some(HexadecimalColorValue::from(value.to_owned()));
    ColorContent::from(node)
}

/// A scheme of black, white, 0E2841, E8E8E8, 156082 and distinct others, in the scheme's order.
fn scheme() -> ColorScheme {
    let mut scheme = ColorScheme::default();
    *scheme.dark_1.content = hex("000000");
    *scheme.light_1.content = hex("FFFFFF");
    *scheme.dark_2.content = hex("0E2841");
    *scheme.light_2.content = hex("E8E8E8");
    *scheme.accent_1.content = hex("156082");
    *scheme.accent_2.content = hex("E97132");
    *scheme.accent_3.content = hex("196B24");
    *scheme.accent_4.content = hex("0F9ED5");
    *scheme.accent_5.content = hex("A02B93");
    *scheme.accent_6.content = hex("4EA72E");
    *scheme.hyperlink.content = hex("467886");
    *scheme.followed_hyperlink.content = hex("96607D");
    scheme
}

fn themed() -> ThemeColors {
    ThemeColors::new(Some(&scheme()), SchemeMap::default())
}

#[test]
fn the_four_types_give_the_same_value() {
    macro_rules! stated {
        ($node:ty) => {{
            let mut node = <$node>::default();
            node.auto = Some(Box::from("1"));
            node.indexed = Some(Box::from("10"));
            node.rgb = Some(HexUnsignedInteger::from("FF102030".to_owned()));
            node.theme = Some(Box::from("4"));
            node.tint = Some(Box::from("-0.25"));
            node
        }};
    }
    let data_bar = stated!(DataBarColor);
    let foreground = stated!(ForegroundColor);
    let background = stated!(BackgroundColor);
    let tab = stated!(SheetTabColor);
    let expected = SheetColor::from(&data_bar);
    assert!(expected.auto);
    assert_eq!(expected.indexed, Some(10));
    assert_eq!(expected.rgb, Some([0x10, 0x20, 0x30]));
    assert_eq!(expected.theme, Some(4));
    assert_eq!(expected.tint, Some("-0.25"));
    assert_eq!(SheetColor::from(&foreground), expected);
    assert_eq!(SheetColor::from(&background), expected);
    assert_eq!(SheetColor::from(&tab), expected);
}

#[test]
fn a_value_that_is_no_number_of_its_type_is_read_as_absent() {
    let mut node = element(Some("FFF"), Some("one"), Some("-1"), None);
    node.auto = Some(Box::from("yes"));
    assert_eq!(SheetColor::from(&node), SheetColor::default());
    assert_eq!(
        SheetColor::from(&element(Some("GG0000"), None, None, None)).rgb,
        None
    );
}

#[test]
fn rgb_is_read_from_its_last_six_digits_and_is_opaque() {
    for value in ["FFFF0000", "00FF0000", "FF0000"] {
        let node = element(Some(value), None, None, None);
        assert_eq!(of(&node, &ThemeColors::default()), Some(opaque(255, 0, 0)));
    }
}

#[test]
fn rgb_comes_before_theme_and_theme_before_indexed() {
    let both = element(Some("00123456"), Some("4"), None, None);
    assert_eq!(of(&both, &themed()), Some(opaque(0x12, 0x34, 0x56)));
    let themed_and_indexed = element(None, Some("4"), Some("2"), None);
    assert_eq!(
        of(&themed_and_indexed, &themed()),
        Some(opaque(0x15, 0x60, 0x82))
    );
    assert_eq!(of(&themed_and_indexed, &ThemeColors::default()), None);
}

#[test]
fn an_element_that_states_none_of_the_three_names_no_colour() {
    let mut auto = DataBarColor::default();
    auto.auto = Some(Box::from("1"));
    assert_eq!(of(&auto, &themed()), None);
    assert_eq!(of(&DataBarColor::default(), &themed()), None);
}

#[test]
fn the_standard_s_palette_gives_indexes_0_to_63() {
    let palette = Palette::new(None);
    assert_eq!(palette.get(0), Some([0, 0, 0]));
    assert_eq!(palette.get(1), Some([255, 255, 255]));
    assert_eq!(palette.get(10), Some([255, 0, 0]));
    assert_eq!(palette.get(22), Some([0xC0, 0xC0, 0xC0]));
    assert_eq!(palette.get(63), Some([0x33, 0x33, 0x33]));
    for system_or_past in [64, 65, 99] {
        assert_eq!(palette.get(system_or_past), None);
    }
    let node = element(None, None, Some("10"), None);
    assert_eq!(of(&node, &ThemeColors::default()), Some(opaque(255, 0, 0)));
}

#[test]
fn a_workbook_s_own_palette_replaces_the_standard_s() {
    let mut indexes = ColorIndexes::default();
    for value in ["FF010101", "FF020202", "FF030303"] {
        let mut entry = RGBColor::default();
        entry.rgb = Some(HexUnsignedInteger::from(value.to_owned()));
        indexes.rgb_color.push(entry);
    }
    let mut colors = Colors::default();
    colors.color_indexes = Some(Box::new(indexes));
    let palette = Palette::new(Some(&colors));
    assert_eq!(palette.get(1), Some([2, 2, 2]));
    assert_eq!(palette.get(5), None);
    assert_eq!(palette.get(64), None);
    assert_eq!(
        Palette::new(Some(&Colors::default())).get(1),
        Some([255; 3])
    );
}

/// What indexes 63 to 66 name under `palette`, with `tint`.
fn past_the_table(palette: &Palette, tint: Option<&str>) -> [Option<Resolved>; 4] {
    ["63", "64", "65", "66"].map(|index| {
        let node = element(None, None, Some(index), tint);
        resolved(&SheetColor::from(&node), &ThemeColors::default(), palette)
    })
}

#[test]
fn indexes_64_and_65_are_the_system_foreground_and_background() {
    assert_eq!(
        past_the_table(&Palette::default(), None),
        [
            Some(Resolved::Color(opaque(0x33, 0x33, 0x33))),
            Some(Resolved::SystemForeground),
            Some(Resolved::SystemBackground),
            None,
        ]
    );
    for system in ["64", "65"] {
        let node = element(None, None, Some(system), None);
        assert_eq!(of(&node, &ThemeColors::default()), None, "{system}");
    }
}

#[test]
fn a_workbook_s_own_palette_does_not_change_the_system_colours() {
    let mut indexes = ColorIndexes::default();
    for _ in 0_u8..67 {
        let mut entry = RGBColor::default();
        entry.rgb = Some(HexUnsignedInteger::from("FF0A0B0C".to_owned()));
        indexes.rgb_color.push(entry);
    }
    let mut colors = Colors::default();
    colors.color_indexes = Some(Box::new(indexes));
    assert_eq!(
        past_the_table(&Palette::new(Some(&colors)), None),
        [
            Some(Resolved::Color(opaque(10, 11, 12))),
            Some(Resolved::SystemForeground),
            Some(Resolved::SystemBackground),
            None,
        ]
    );
}

#[test]
fn rgb_and_theme_come_before_a_system_colour() {
    let stated = element(Some("FF123456"), None, Some("64"), None);
    assert_eq!(
        resolved(&SheetColor::from(&stated), &themed(), &Palette::default()),
        Some(Resolved::Color(opaque(0x12, 0x34, 0x56)))
    );
    let themed_and_system = element(None, Some("4"), Some("65"), None);
    assert_eq!(
        resolved(
            &SheetColor::from(&themed_and_system),
            &themed(),
            &Palette::default()
        ),
        Some(Resolved::Color(opaque(0x15, 0x60, 0x82)))
    );
}

#[test]
fn a_system_colour_s_tint_is_applied_to_the_caller_s_colour() {
    assert_eq!(
        past_the_table(&Palette::default(), Some("-0.5")),
        [
            Some(Resolved::Color(opaque(26, 26, 26))),
            Some(Resolved::SystemForeground),
            Some(Resolved::SystemBackground),
            None,
        ]
    );
    let node = element(None, None, Some("65"), Some("-0.5"));
    let stated = SheetColor::from(&node);
    assert_eq!(stated.with_tint([200, 200, 200]), opaque(100, 100, 100));
    assert_eq!(
        SheetColor::from(&element(None, None, Some("64"), None)).with_tint([1, 2, 3]),
        opaque(1, 2, 3)
    );
}

#[test]
fn a_theme_index_counts_the_first_two_pairs_turned_about() {
    let expected = [
        (0_u32, Some(opaque(255, 255, 255))),
        (1, Some(opaque(0, 0, 0))),
        (2, Some(opaque(0xE8, 0xE8, 0xE8))),
        (3, Some(opaque(0x0E, 0x28, 0x41))),
        (4, Some(opaque(0x15, 0x60, 0x82))),
        (10, Some(opaque(0x46, 0x78, 0x86))),
        (11, Some(opaque(0x96, 0x60, 0x7D))),
        (12, None),
    ];
    let theme = themed();
    for (index, rgba) in expected {
        let node = element(None, Some(&index.to_string()), None, None);
        assert_eq!(of(&node, &theme), rgba, "theme index {index}");
    }
    assert_eq!(theme_slot(9), Some(ThemeSlot::Accent6));
    assert_eq!(theme_slot(12), None);
}

#[test]
fn a_theme_index_with_no_theme_names_no_colour() {
    let node = element(None, Some("1"), None, None);
    assert_eq!(of(&node, &ThemeColors::default()), None);
}

#[test]
fn a_tint_above_0_lightens_with_the_hue_and_saturation_kept() {
    assert_eq!(tinted("156082", "0.5"), Some(opaque(100, 190, 230)));
    let node = element(None, Some("4"), None, Some("0.5"));
    assert_eq!(of(&node, &themed()), Some(opaque(100, 190, 230)));
}

#[test]
fn a_tint_below_0_darkens_each_component_in_proportion() {
    assert_eq!(tinted("156082", "-0.5"), Some(opaque(11, 48, 65)));
}

#[test]
fn the_standard_s_examples_hold_by_its_rule() {
    assert_eq!(tinted("C8C8C8", "-0.5"), Some(opaque(100, 100, 100)));
    assert_eq!(tinted("C8C8C8", "-1.0"), Some(opaque(0, 0, 0)));
    // 100 × 0.25 + 255 × 0.75 is 216.25; the example's 217 subtracts 63 for 63.75.
    assert_eq!(tinted("646464", "0.75"), Some(opaque(216, 216, 216)));
    assert_eq!(tinted("646464", "1.0"), Some(opaque(255, 255, 255)));
}

#[test]
fn a_tint_as_excel_writes_it_gives_a_colour() {
    assert_eq!(
        tinted("FFFFFF", "-0.249977111117893"),
        Some(opaque(191, 191, 191))
    );
    assert_eq!(
        tinted("000000", "0.39997558519241921"),
        Some(opaque(102, 102, 102))
    );
    assert_eq!(
        tinted("000000", "-0.249977111117893"),
        Some(opaque(0, 0, 0))
    );
    assert_eq!(
        tinted("FFFFFF", "0.39997558519241921"),
        Some(opaque(255, 255, 255))
    );
    assert_eq!(
        tinted("156082", "-4.9989318521683403E-2"),
        tinted("156082", "-0.049989318521683403")
    );
    assert_eq!(tinted("156082", "5E-1"), Some(opaque(100, 190, 230)));
}

#[test]
fn a_tint_that_is_no_number_changes_nothing_and_one_past_1_is_held_at_1() {
    for nonsense in ["nonsense", "", ".", "-", "1.2.3", "NaN", "INF", "0x1"] {
        assert_eq!(tinted("156082", nonsense), Some(opaque(0x15, 0x60, 0x82)));
    }
    assert_eq!(tinted("156082", "0"), Some(opaque(0x15, 0x60, 0x82)));
    assert_eq!(tinted("156082", "3"), Some(opaque(255, 255, 255)));
    assert_eq!(tinted("156082", "1e400"), Some(opaque(255, 255, 255)));
    assert_eq!(tinted("156082", "-3"), Some(opaque(0, 0, 0)));
    assert_eq!(tinted("156082", "1e-400"), Some(opaque(0x15, 0x60, 0x82)));
}

#[test]
fn a_workbook_gives_its_theme_s_colours_and_the_standard_s_palette() {
    let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    let new_theme = xlsx.theme_colors().unwrap();
    let new_palette = xlsx.palette().unwrap();
    for index in 0_u32..12 {
        let node = element(None, Some(&index.to_string()), None, None);
        assert_eq!(
            color(&SheetColor::from(&node), &new_theme, &new_palette),
            None
        );
    }
    assert_eq!(new_palette.get(10), Some([255, 0, 0]));

    let mut theme_root = Theme::default();
    *theme_root.elements.color_scheme = scheme();
    add(
        &mut xlsx,
        "/xl/theme/theme1.xml",
        theme::CONTENT_TYPE,
        &theme_root,
        &theme::RELATIONSHIP_TYPE,
    );
    add(
        &mut xlsx,
        "/xl/styles.xml",
        styles::CONTENT_TYPE,
        &StyleSheet::default(),
        &styles::RELATIONSHIP_TYPE,
    );
    let theme = xlsx.theme_colors().unwrap();
    let palette = xlsx.palette().unwrap();
    let node = element(None, Some("1"), None, None);
    assert_eq!(
        color(&SheetColor::from(&node), &theme, &palette),
        Some(opaque(0, 0, 0))
    );
    let accent = element(None, Some("4"), None, Some("0.5"));
    assert_eq!(
        color(&SheetColor::from(&accent), &theme, &palette),
        Some(opaque(100, 190, 230))
    );
    assert_eq!(palette.get(22), Some([0xC0, 0xC0, 0xC0]));
}
