#[cfg(feature = "transitional")]
use crate::parts::kinds::PresentationKind;
#[cfg(feature = "transitional")]
use crate::parts::{slide, slide_layout, slide_master, theme};
use officekit_markup::common_simple_types::HexadecimalColorValue;
use officekit_markup::drawing::{
    ColorContent, ColorSchemeMapOverrideContent, MasterColorMapping, OverrideColorMapping,
    RGBColorModelHexVariant, StSchemeColorValMember, Theme, ThemeColorReference,
    ThemeColorReferenceMember,
};
use officekit_markup::presentation::{
    ColorSchemeMap, ColorSchemeMapOverride, PresentationSlideSlide, SlideLayout, SlideMaster,
};
#[cfg(feature = "transitional")]
use officekit_markup::presentation::{ListOfSlideIDs, SlideID};
#[cfg(feature = "transitional")]
use officekit_xml::packaging::package::OpenOptions;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::part_name::PartName;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::relationship::{RelationshipType, Target};
#[cfg(feature = "transitional")]
use officekit_xml::write::WriteOptions;

use super::{in_force, theme_colors};
use crate::drawing::scheme::ThemeSlot;
#[cfg(feature = "transitional")]
use crate::pptx::Pptx;
#[cfg(feature = "transitional")]
use crate::pptx::slides::SlideParts;

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

/// An `a:srgbClr` of `text`.
fn hex(text: &str) -> ColorContent {
    let mut color = RGBColorModelHexVariant::default();
    color.val = Some(HexadecimalColorValue::from(text.to_owned()));
    ColorContent::from(color)
}

/// A theme whose twelve colours differ.
fn theme_with_twelve_colours() -> Theme {
    let mut found = Theme::default();
    let scheme = &mut found.elements.color_scheme;
    *scheme.accent_1.content = hex("A00001");
    *scheme.accent_2.content = hex("A00002");
    *scheme.accent_3.content = hex("A00003");
    *scheme.accent_4.content = hex("A00004");
    *scheme.accent_5.content = hex("A00005");
    *scheme.accent_6.content = hex("A00006");
    *scheme.dark_1.content = hex("000000");
    *scheme.dark_2.content = hex("101010");
    *scheme.followed_hyperlink.content = hex("F0F0F0");
    *scheme.hyperlink.content = hex("0000FF");
    *scheme.light_1.content = hex("FFFFFF");
    *scheme.light_2.content = hex("EEEEEE");
    found
}

/// A `p:clrMapOvr` holding `mapping` as its `a:overrideClrMapping`.
fn overriding(mapping: OverrideColorMapping) -> ColorSchemeMapOverride {
    let mut map = ColorSchemeMapOverride::default();
    map.content = Box::new(ColorSchemeMapOverrideContent::OverrideColorMapping(
        Box::new(mapping),
    ));
    map
}

/// A `p:clrMapOvr` holding an `a:overrideClrMapping` that states the colour of `tx1` alone.
fn overriding_tx1(tx1: ThemeColorReferenceMember) -> ColorSchemeMapOverride {
    let mut mapping = OverrideColorMapping::default();
    mapping.tx1 = Some(ThemeColorReference::from(tx1));
    overriding(mapping)
}

/// A `p:clrMapOvr` holding `a:masterClrMapping`.
fn master_mapping() -> ColorSchemeMapOverride {
    let mut map = ColorSchemeMapOverride::default();
    map.content = Box::new(ColorSchemeMapOverrideContent::from(
        MasterColorMapping::default(),
    ));
    map
}

/// A slide master whose `p:clrMap` states the colours of the names `tx1` and `bg1` and no other.
fn master_with(tx1: ThemeColorReferenceMember, bg1: ThemeColorReferenceMember) -> SlideMaster {
    let mut map = ColorSchemeMap::default();
    map.tx1 = Some(ThemeColorReference::from(tx1));
    map.bg1 = Some(ThemeColorReference::from(bg1));
    let mut master = SlideMaster::default();
    master.color_scheme_map = Box::new(map);
    master
}

#[test]
fn a_slide_that_maps_accent1_onto_accent2_gives_the_theme_s_second_accent() {
    let mut mapping = OverrideColorMapping::default();
    mapping.accent1 = Some(ThemeColorReference::from(
        ThemeColorReferenceMember::Accent2,
    ));
    let mut slide_root = PresentationSlideSlide::default();
    slide_root.color_scheme_map_override = Some(Box::new(overriding(mapping)));
    let found = theme_with_twelve_colours();
    let colors = theme_colors(
        &slide_root,
        &SlideLayout::default(),
        &SlideMaster::default(),
        Some(&found),
    );
    assert_eq!(
        colors.color(StSchemeColorValMember::Accent1),
        Some(&*found.elements.color_scheme.accent_2.content)
    );
}

#[test]
fn a_slide_with_master_clr_mapping_under_a_layout_with_its_own_map_takes_the_layout_s() {
    let mut slide_root = PresentationSlideSlide::default();
    slide_root.color_scheme_map_override = Some(Box::new(master_mapping()));
    let mut layout = SlideLayout::default();
    layout.color_scheme_map_override =
        Some(Box::new(overriding_tx1(ThemeColorReferenceMember::Lt1)));
    let master = master_with(
        ThemeColorReferenceMember::Dk1,
        ThemeColorReferenceMember::Lt1,
    );
    assert_eq!(
        in_force(&slide_root, &layout, &master).tx1,
        ThemeSlot::Light1
    );
}

#[test]
fn a_slide_with_no_clr_map_ovr_takes_the_layout_s_map() {
    let mut layout = SlideLayout::default();
    layout.color_scheme_map_override =
        Some(Box::new(overriding_tx1(ThemeColorReferenceMember::Lt1)));
    let master = master_with(
        ThemeColorReferenceMember::Dk1,
        ThemeColorReferenceMember::Lt1,
    );
    assert_eq!(
        in_force(&PresentationSlideSlide::default(), &layout, &master).tx1,
        ThemeSlot::Light1
    );
}

#[test]
fn with_neither_slide_nor_layout_overriding_the_master_s_map_is_in_force() {
    let master = master_with(
        ThemeColorReferenceMember::Lt1,
        ThemeColorReferenceMember::Dk1,
    );
    for slide_map in [None, Some(Box::new(master_mapping()))] {
        for layout_map in [None, Some(Box::new(master_mapping()))] {
            let mut slide_root = PresentationSlideSlide::default();
            slide_root.color_scheme_map_override = slide_map.clone();
            let mut layout = SlideLayout::default();
            layout.color_scheme_map_override = layout_map;
            let map = in_force(&slide_root, &layout, &master);
            assert_eq!(map.bg1, ThemeSlot::Dark1);
            assert_eq!(map.tx1, ThemeSlot::Light1);
        }
    }
}

#[test]
fn a_slide_s_own_map_is_in_force_over_the_layout_s() {
    let mut slide_root = PresentationSlideSlide::default();
    slide_root.color_scheme_map_override =
        Some(Box::new(overriding_tx1(ThemeColorReferenceMember::Lt2)));
    let mut layout = SlideLayout::default();
    layout.color_scheme_map_override =
        Some(Box::new(overriding_tx1(ThemeColorReferenceMember::Lt1)));
    let master = master_with(
        ThemeColorReferenceMember::Dk1,
        ThemeColorReferenceMember::Lt1,
    );
    assert_eq!(
        in_force(&slide_root, &layout, &master).tx1,
        ThemeSlot::Light2
    );
}

#[test]
fn with_no_theme_no_name_has_a_colour_and_the_map_in_force_stands() {
    let mut slide_root = PresentationSlideSlide::default();
    slide_root.color_scheme_map_override =
        Some(Box::new(overriding_tx1(ThemeColorReferenceMember::Lt2)));
    let master = master_with(
        ThemeColorReferenceMember::Dk1,
        ThemeColorReferenceMember::Dk2,
    );
    let layout = SlideLayout::default();
    let colors = theme_colors(&slide_root, &layout, &master, None);
    for name in NAMES {
        assert_eq!(colors.color(name), None, "{name:?}");
    }
    assert_eq!(colors.map(), &in_force(&slide_root, &layout, &master));
    assert_eq!(colors.map().tx1, ThemeSlot::Light2);
}

/// The first part `source` relates to by `relationship_type`.
#[cfg(feature = "transitional")]
fn target(pptx: &Pptx, source: &PartName, relationship_type: &RelationshipType) -> PartName {
    pptx.package()
        .related_by_type(Some(source), relationship_type)
        .unwrap()
        .into_iter()
        .find_map(|related| {
            if let Target::Part(part) = related.target {
                Some(part)
            } else {
                None
            }
        })
        .unwrap()
}

/// Store an empty slide related to `layout` and make it the presentation's one listed slide;
/// returns its entry of [`Pptx::slides`].
#[cfg(feature = "transitional")]
fn one_slide_on(pptx: &mut Pptx, layout: &PartName) -> SlideParts {
    let main = pptx.presentation_part().clone();
    let slide_part = PartName::new("/ppt/slides/slide1.xml").unwrap();
    slide::store_transitional(
        pptx.package_mut(),
        &slide_part,
        &PresentationSlideSlide::default(),
        WriteOptions::default(),
    )
    .unwrap();
    let id = pptx
        .package_mut()
        .relate(
            Some(&main),
            &slide_part,
            slide::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    pptx.package_mut()
        .relate(
            Some(&slide_part),
            layout,
            slide_layout::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    let mut entry = SlideID::default();
    entry.id = Some("256".to_owned().into());
    entry.r_id = Some(id.into());
    let mut list = ListOfSlideIDs::default();
    list.slide_id.push(entry);
    let mut presentation = pptx.presentation().unwrap();
    presentation.list_of_slide_ids = Some(Box::new(list));
    pptx.set_presentation(&presentation).unwrap();
    let [parts] = pptx.slides().unwrap().try_into().unwrap();
    parts
}

#[cfg(feature = "transitional")]
#[test]
fn slide_colors_maps_the_names_of_a_slide_of_a_file_through_its_master_s_map() {
    let mut pptx = Pptx::new(PresentationKind::Presentation).unwrap();
    let main = pptx.presentation_part().clone();
    let master_part = target(&pptx, &main, &slide_master::RELATIONSHIP_TYPE);
    let layout_part = target(&pptx, &master_part, &slide_layout::RELATIONSHIP_TYPE);
    let theme_part = target(&pptx, &master_part, &theme::RELATIONSHIP_TYPE);

    let mut master =
        slide_master::load(pptx.package(), &master_part, OpenOptions::default()).unwrap();
    master.color_scheme_map.tx1 = Some(ThemeColorReference::from(ThemeColorReferenceMember::Lt2));
    slide_master::store_transitional(
        pptx.package_mut(),
        &master_part,
        &master,
        WriteOptions::default(),
    )
    .unwrap();
    theme::store_transitional(
        pptx.package_mut(),
        &theme_part,
        &theme_with_twelve_colours(),
        WriteOptions::default(),
    )
    .unwrap();

    let parts = one_slide_on(&mut pptx, &layout_part);
    let colors = pptx.slide_colors(&parts).unwrap();
    let stored = theme::load(pptx.package(), &theme_part, OpenOptions::default()).unwrap();
    assert_eq!(colors.map().tx1, ThemeSlot::Light2);
    assert_eq!(
        colors.color(StSchemeColorValMember::Tx1),
        Some(&*stored.elements.color_scheme.light_2.content)
    );
    assert_eq!(
        colors.color(StSchemeColorValMember::Tx1),
        Some(&hex("EEEEEE"))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn slide_colors_of_a_slide_whose_master_has_no_theme_gives_no_colour_under_the_master_s_map() {
    let mut pptx = Pptx::new(PresentationKind::Presentation).unwrap();
    let master_part = PartName::new("/ppt/slideMasters/slideMaster2.xml").unwrap();
    let layout_part = PartName::new("/ppt/slideLayouts/slideLayout2.xml").unwrap();
    slide_master::store_transitional(
        pptx.package_mut(),
        &master_part,
        &master_with(
            ThemeColorReferenceMember::Lt2,
            ThemeColorReferenceMember::Dk2,
        ),
        WriteOptions::default(),
    )
    .unwrap();
    slide_layout::store_transitional(
        pptx.package_mut(),
        &layout_part,
        &SlideLayout::default(),
        WriteOptions::default(),
    )
    .unwrap();
    pptx.package_mut()
        .relate(
            Some(&layout_part),
            &master_part,
            slide_master::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();

    let parts = one_slide_on(&mut pptx, &layout_part);
    assert_eq!(parts.master, master_part);
    assert_eq!(parts.theme, None);
    let colors = pptx.slide_colors(&parts).unwrap();
    for name in NAMES {
        assert_eq!(colors.color(name), None, "{name:?}");
    }
    let options = OpenOptions::default();
    let expected = in_force(
        &slide::load(pptx.package(), &parts.slide, options).unwrap(),
        &slide_layout::load(pptx.package(), &layout_part, options).unwrap(),
        &slide_master::load(pptx.package(), &master_part, options).unwrap(),
    );
    assert_eq!(colors.map(), &expected);
    assert_eq!(colors.map().tx1, ThemeSlot::Light2);
    assert_eq!(colors.map().bg1, ThemeSlot::Dark2);
}
