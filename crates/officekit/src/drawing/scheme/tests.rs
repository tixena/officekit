use officekit_markup::common_simple_types::HexadecimalColorValue;
use officekit_markup::drawing::{
    ColorContent, ColorScheme, OverrideColorMapping, RGBColorModelHexVariant,
    StSchemeColorValMember, ThemeColorReference as ColorSchemeIndex,
    ThemeColorReferenceMember as ColorSchemeIndexMember,
};
use officekit_markup::presentation::ColorSchemeMap;
use officekit_markup::wordprocessing::{
    ThemeColorMappings, ThemeColorReference, ThemeColorReferenceMember,
};

use super::{SchemeMap, ThemeColors, ThemeSlot};

/// Every member of `ST_SchemeColorVal`.
const NAMES: [StSchemeColorValMember; 17] = [
    StSchemeColorValMember::Accent1,
    StSchemeColorValMember::Accent2,
    StSchemeColorValMember::Accent3,
    StSchemeColorValMember::Accent4,
    StSchemeColorValMember::Accent5,
    StSchemeColorValMember::Accent6,
    StSchemeColorValMember::Bg1,
    StSchemeColorValMember::Bg2,
    StSchemeColorValMember::Dk1,
    StSchemeColorValMember::Dk2,
    StSchemeColorValMember::FolHlink,
    StSchemeColorValMember::Hlink,
    StSchemeColorValMember::Lt1,
    StSchemeColorValMember::Lt2,
    StSchemeColorValMember::PhClr,
    StSchemeColorValMember::Tx1,
    StSchemeColorValMember::Tx2,
];

/// Every theme colour.
const SLOTS: [ThemeSlot; 12] = [
    ThemeSlot::Accent1,
    ThemeSlot::Accent2,
    ThemeSlot::Accent3,
    ThemeSlot::Accent4,
    ThemeSlot::Accent5,
    ThemeSlot::Accent6,
    ThemeSlot::Dark1,
    ThemeSlot::Dark2,
    ThemeSlot::FollowedHyperlink,
    ThemeSlot::Hyperlink,
    ThemeSlot::Light1,
    ThemeSlot::Light2,
];

/// An `a:srgbClr` of `text`.
fn hex(text: &str) -> ColorContent {
    let mut color = RGBColorModelHexVariant::default();
    color.val = Some(HexadecimalColorValue::from(text.to_owned()));
    ColorContent::RGBColorModelHexVariant(color)
}

/// A colour scheme whose twelve colours differ.
fn scheme() -> ColorScheme {
    let mut scheme = ColorScheme::default();
    scheme.accent_1.content = Box::new(hex("A00001"));
    scheme.accent_2.content = Box::new(hex("A00002"));
    scheme.accent_3.content = Box::new(hex("A00003"));
    scheme.accent_4.content = Box::new(hex("A00004"));
    scheme.accent_5.content = Box::new(hex("A00005"));
    scheme.accent_6.content = Box::new(hex("A00006"));
    scheme.dark_1.content = Box::new(hex("000000"));
    scheme.dark_2.content = Box::new(hex("101010"));
    scheme.followed_hyperlink.content = Box::new(hex("F0F0F0"));
    scheme.hyperlink.content = Box::new(hex("0000FF"));
    scheme.light_1.content = Box::new(hex("FFFFFF"));
    scheme.light_2.content = Box::new(hex("EEEEEE"));
    scheme
}

/// A `w:clrSchemeMapping` whose `w:t1` is `t1` and that states nothing else.
fn t1_mapping(t1: ThemeColorReference) -> ThemeColorMappings {
    let mut mapping = ThemeColorMappings::default();
    mapping.t1 = Some(t1);
    mapping
}

#[test]
fn with_no_mapping_stated_the_text_and_background_names_are_the_dark_and_light_colours() {
    let theme = scheme();
    let colors = ThemeColors::new(Some(&theme), SchemeMap::default());
    assert_eq!(
        colors.color(StSchemeColorValMember::Tx1),
        Some(&*theme.dark_1.content)
    );
    assert_eq!(
        colors.color(StSchemeColorValMember::Bg1),
        Some(&*theme.light_1.content)
    );
    assert_eq!(
        colors.color(StSchemeColorValMember::Tx2),
        Some(&*theme.dark_2.content)
    );
    assert_eq!(
        colors.color(StSchemeColorValMember::Bg2),
        Some(&*theme.light_2.content)
    );
}

#[test]
fn with_no_mapping_stated_each_accent_and_hyperlink_name_is_its_own_colour() {
    let theme = scheme();
    let colors = ThemeColors::new(Some(&theme), SchemeMap::default());
    let own = [
        (StSchemeColorValMember::Accent1, &theme.accent_1.content),
        (StSchemeColorValMember::Accent2, &theme.accent_2.content),
        (StSchemeColorValMember::Accent3, &theme.accent_3.content),
        (StSchemeColorValMember::Accent4, &theme.accent_4.content),
        (StSchemeColorValMember::Accent5, &theme.accent_5.content),
        (StSchemeColorValMember::Accent6, &theme.accent_6.content),
        (
            StSchemeColorValMember::FolHlink,
            &theme.followed_hyperlink.content,
        ),
        (StSchemeColorValMember::Hlink, &theme.hyperlink.content),
    ];
    for (name, content) in own {
        assert_eq!(colors.color(name), Some(&**content), "{name:?}");
    }
}

#[test]
fn a_mapping_of_t1_to_light1_gives_tx1_the_first_light_colour() {
    let theme = scheme();
    let map = SchemeMap::from_word(&t1_mapping(ThemeColorReference::from(
        ThemeColorReferenceMember::Light1,
    )));
    let colors = ThemeColors::new(Some(&theme), map);
    assert_eq!(colors.map().tx1, ThemeSlot::Light1);
    assert_eq!(
        colors.color(StSchemeColorValMember::Tx1),
        Some(&*theme.light_1.content)
    );
}

#[test]
fn a_theme_colour_named_itself_goes_through_no_map() {
    let theme = scheme();
    let map = SchemeMap::from_word(&t1_mapping(ThemeColorReference::from(
        ThemeColorReferenceMember::Light1,
    )));
    let colors = ThemeColors::new(Some(&theme), map);
    assert_eq!(
        colors.color(StSchemeColorValMember::Dk1),
        Some(&*theme.dark_1.content)
    );
    assert_eq!(
        colors.color(StSchemeColorValMember::Lt1),
        Some(&*theme.light_1.content)
    );
    assert_eq!(
        colors.color(StSchemeColorValMember::Dk2),
        Some(&*theme.dark_2.content)
    );
    assert_eq!(
        colors.color(StSchemeColorValMember::Lt2),
        Some(&*theme.light_2.content)
    );
}

#[test]
fn an_accent_mapped_onto_another_gives_the_other_accent() {
    let theme = scheme();
    let mut mapping = ThemeColorMappings::default();
    mapping.accent1 = Some(ThemeColorReference::from(
        ThemeColorReferenceMember::Accent2,
    ));
    let colors = ThemeColors::new(Some(&theme), SchemeMap::from_word(&mapping));
    assert_eq!(
        colors.color(StSchemeColorValMember::Accent1),
        Some(&*theme.accent_2.content)
    );
}

#[test]
fn each_attribute_of_a_word_mapping_maps_its_own_name() {
    let mut mapping = ThemeColorMappings::default();
    mapping.accent1 = Some(ThemeColorReference::from(ThemeColorReferenceMember::Light2));
    mapping.accent2 = Some(ThemeColorReference::from(ThemeColorReferenceMember::Light1));
    mapping.accent3 = Some(ThemeColorReference::from(
        ThemeColorReferenceMember::Hyperlink,
    ));
    mapping.accent4 = Some(ThemeColorReference::from(
        ThemeColorReferenceMember::FollowedHyperlink,
    ));
    mapping.accent5 = Some(ThemeColorReference::from(ThemeColorReferenceMember::Dark2));
    mapping.accent6 = Some(ThemeColorReference::from(ThemeColorReferenceMember::Dark1));
    mapping.bg1 = Some(ThemeColorReference::from(
        ThemeColorReferenceMember::Accent6,
    ));
    mapping.bg2 = Some(ThemeColorReference::from(
        ThemeColorReferenceMember::Accent5,
    ));
    mapping.followed_hyperlink = Some(ThemeColorReference::from(
        ThemeColorReferenceMember::Accent4,
    ));
    mapping.hyperlink = Some(ThemeColorReference::from(
        ThemeColorReferenceMember::Accent3,
    ));
    mapping.t1 = Some(ThemeColorReference::from(
        ThemeColorReferenceMember::Accent2,
    ));
    mapping.t2 = Some(ThemeColorReference::from(
        ThemeColorReferenceMember::Accent1,
    ));
    let expected = SchemeMap {
        accent1: ThemeSlot::Light2,
        accent2: ThemeSlot::Light1,
        accent3: ThemeSlot::Hyperlink,
        accent4: ThemeSlot::FollowedHyperlink,
        accent5: ThemeSlot::Dark2,
        accent6: ThemeSlot::Dark1,
        bg1: ThemeSlot::Accent6,
        bg2: ThemeSlot::Accent5,
        fol_hlink: ThemeSlot::Accent4,
        hlink: ThemeSlot::Accent3,
        tx1: ThemeSlot::Accent2,
        tx2: ThemeSlot::Accent1,
    };
    assert_eq!(SchemeMap::from_word(&mapping), expected);
}

#[test]
fn a_word_file_that_states_no_mapping_has_the_default_one() {
    assert_eq!(
        SchemeMap::from_word(&ThemeColorMappings::default()),
        SchemeMap::default()
    );
}

#[test]
fn ph_clr_names_no_colour() {
    let theme = scheme();
    let colors = ThemeColors::new(Some(&theme), SchemeMap::default());
    assert_eq!(
        SchemeMap::default().slot(StSchemeColorValMember::PhClr),
        None
    );
    assert_eq!(colors.color(StSchemeColorValMember::PhClr), None);
}

#[test]
fn a_mapping_value_that_is_no_member_keeps_the_default() {
    let theme = scheme();
    let map = SchemeMap::from_word(&t1_mapping(ThemeColorReference::from(
        "nonsense".to_owned(),
    )));
    assert_eq!(map.tx1, ThemeSlot::Dark1);
    let colors = ThemeColors::new(Some(&theme), map);
    assert_eq!(
        colors.color(StSchemeColorValMember::Tx1),
        Some(&*theme.dark_1.content)
    );
}

#[test]
fn with_no_theme_no_name_has_a_colour() {
    let map = SchemeMap::from_word(&t1_mapping(ThemeColorReference::from(
        ThemeColorReferenceMember::Light1,
    )));
    for colors in [ThemeColors::default(), ThemeColors::new(None, map)] {
        for name in NAMES {
            assert_eq!(colors.color(name), None, "{name:?}");
        }
        for slot in SLOTS {
            assert_eq!(colors.slot(slot), None, "{slot:?}");
        }
    }
}

/// A `p:clrMap` whose twelve attributes each name a colour other than their own.
fn master_map() -> ColorSchemeMap {
    let mut map = ColorSchemeMap::default();
    map.accent1 = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Lt2));
    map.accent2 = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Lt1));
    map.accent3 = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Hlink));
    map.accent4 = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::FolHlink));
    map.accent5 = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Dk2));
    map.accent6 = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Dk1));
    map.bg1 = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Accent6));
    map.bg2 = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Accent5));
    map.fol_hlink = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Accent4));
    map.hlink = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Accent3));
    map.tx1 = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Accent2));
    map.tx2 = Some(ColorSchemeIndex::from(ColorSchemeIndexMember::Accent1));
    map
}

#[test]
fn each_attribute_of_a_master_s_clr_map_maps_its_own_name() {
    let expected = SchemeMap {
        accent1: ThemeSlot::Light2,
        accent2: ThemeSlot::Light1,
        accent3: ThemeSlot::Hyperlink,
        accent4: ThemeSlot::FollowedHyperlink,
        accent5: ThemeSlot::Dark2,
        accent6: ThemeSlot::Dark1,
        bg1: ThemeSlot::Accent6,
        bg2: ThemeSlot::Accent5,
        fol_hlink: ThemeSlot::Accent4,
        hlink: ThemeSlot::Accent3,
        tx1: ThemeSlot::Accent2,
        tx2: ThemeSlot::Accent1,
    };
    assert_eq!(SchemeMap::from_master(&master_map()), expected);
}

#[test]
fn an_attribute_of_a_clr_map_that_is_absent_or_no_value_keeps_the_default() {
    let mut map = ColorSchemeMap::default();
    map.bg1 = Some(ColorSchemeIndex::from("nonsense".to_owned()));
    let read = SchemeMap::from_master(&map);
    assert_eq!(read.tx1, ThemeSlot::Dark1);
    assert_eq!(read.bg1, ThemeSlot::Light1);
    assert_eq!(read, SchemeMap::default());

    let mut mapping = OverrideColorMapping::default();
    mapping.bg1 = Some(ColorSchemeIndex::from("nonsense".to_owned()));
    assert_eq!(SchemeMap::from_override(&mapping), SchemeMap::default());
}

#[test]
fn a_master_s_clr_map_and_an_override_of_the_same_values_agree() {
    let map = master_map();
    let mut mapping = OverrideColorMapping::default();
    mapping.accent1.clone_from(&map.accent1);
    mapping.accent2.clone_from(&map.accent2);
    mapping.accent3.clone_from(&map.accent3);
    mapping.accent4.clone_from(&map.accent4);
    mapping.accent5.clone_from(&map.accent5);
    mapping.accent6.clone_from(&map.accent6);
    mapping.bg1.clone_from(&map.bg1);
    mapping.bg2.clone_from(&map.bg2);
    mapping.fol_hlink.clone_from(&map.fol_hlink);
    mapping.hlink.clone_from(&map.hlink);
    mapping.tx1.clone_from(&map.tx1);
    mapping.tx2.clone_from(&map.tx2);
    assert_eq!(
        SchemeMap::from_override(&mapping),
        SchemeMap::from_master(&map)
    );
    assert_ne!(SchemeMap::from_master(&map), SchemeMap::default());
}
