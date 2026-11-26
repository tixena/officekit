#[cfg(feature = "transitional")]
use core::array::from_fn;

use crate::parts::kinds::PresentationKind;
#[cfg(feature = "transitional")]
use crate::parts::{slide, slide_layout, slide_master, theme};
#[cfg(feature = "transitional")]
use officekit_markup::presentation::{
    ListOfSlideIDs, PresentationSlideSlide, SlideID, SlideLayout, SlideMaster,
};
#[cfg(feature = "transitional")]
use officekit_xml::packaging::error::PackageError;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::part_name::PartName;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::relationship::{RelationshipType, Target};
#[cfg(feature = "transitional")]
use officekit_xml::write::WriteOptions;

#[cfg(feature = "transitional")]
use super::SlideParts;
use crate::pptx::Pptx;

/// The master, layout and theme parts a new presentation holds.
#[cfg(feature = "transitional")]
struct Template {
    layout: PartName,
    master: PartName,
    theme: PartName,
}

#[cfg(feature = "transitional")]
fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
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

/// The master, layout and theme a new presentation holds, found through its relationships.
#[cfg(feature = "transitional")]
fn template(pptx: &Pptx) -> Template {
    let master = target(
        pptx,
        pptx.presentation_part(),
        &slide_master::RELATIONSHIP_TYPE,
    );
    Template {
        layout: target(pptx, &master, &slide_layout::RELATIONSHIP_TYPE),
        theme: target(pptx, &master, &theme::RELATIONSHIP_TYPE),
        master,
    }
}

/// Store an empty slide at `/ppt/slides/slide{number}.xml`, related from the presentation part
/// and to `layout` when there is one; returns its name and the presentation's relationship id.
#[cfg(feature = "transitional")]
fn add_slide(pptx: &mut Pptx, number: usize, layout: Option<&PartName>) -> (PartName, String) {
    let main = pptx.presentation_part().clone();
    let part = name(&format!("/ppt/slides/slide{number}.xml"));
    slide::store_transitional(
        pptx.package_mut(),
        &part,
        &PresentationSlideSlide::default(),
        WriteOptions::default(),
    )
    .unwrap();
    let id = pptx
        .package_mut()
        .relate(Some(&main), &part, slide::RELATIONSHIP_TYPE.transitional)
        .unwrap();
    if let Some(found) = layout {
        pptx.package_mut()
            .relate(
                Some(&part),
                found,
                slide_layout::RELATIONSHIP_TYPE.transitional,
            )
            .unwrap();
    }
    (part, id)
}

/// Set the presentation's `p:sldIdLst` to one entry for each of `r_ids`, in order.
#[cfg(feature = "transitional")]
fn set_list(pptx: &mut Pptx, r_ids: &[Option<&str>]) {
    let mut list = ListOfSlideIDs::default();
    for (number, r_id) in (256_u32..).zip(r_ids) {
        let mut entry = SlideID::default();
        entry.id = Some(number.to_string().into());
        entry.r_id = r_id.map(Box::from);
        list.slide_id.push(entry);
    }
    let mut presentation = pptx.presentation().unwrap();
    presentation.list_of_slide_ids = Some(Box::new(list));
    pptx.set_presentation(&presentation).unwrap();
}

/// The entry of `slide` drawn with the new presentation's layout, master and theme.
#[cfg(feature = "transitional")]
fn drawn_with(slide: &PartName, parts: &Template) -> SlideParts {
    SlideParts {
        layout: parts.layout.clone(),
        master: parts.master.clone(),
        slide: slide.clone(),
        theme: Some(parts.theme.clone()),
    }
}

/// A new presentation with `N` slides, each related to its layout, and the slides' names and
/// relationship ids in order.
#[cfg(feature = "transitional")]
fn with_slides<const N: usize>() -> (Pptx, Template, [(PartName, String); N]) {
    let mut pptx = Pptx::new(PresentationKind::Presentation).unwrap();
    let parts = template(&pptx);
    let added = from_fn(|index| add_slide(&mut pptx, index + 1, Some(&parts.layout)));
    (pptx, parts, added)
}

/// The `PackageError::Read` part and reason of `error`.
#[cfg(feature = "transitional")]
fn read_error(error: PackageError) -> Option<(PartName, String)> {
    if let PackageError::Read { part, reason } = error {
        Some((part, reason))
    } else {
        None
    }
}

#[cfg(feature = "transitional")]
#[test]
fn three_slides_give_three_entries_in_order_with_the_one_layout_master_and_theme() {
    let (mut pptx, parts, added) = with_slides::<3>();
    let ids: Vec<Option<&str>> = added.iter().map(|(_, id)| Some(id.as_str())).collect();
    set_list(&mut pptx, &ids);
    let expected: Vec<SlideParts> = added
        .iter()
        .map(|(part, _)| drawn_with(part, &parts))
        .collect();
    assert_eq!(pptx.slides().unwrap(), expected);
    assert_eq!(
        expected.first().unwrap().slide,
        name("/ppt/slides/slide1.xml")
    );
}

#[cfg(feature = "transitional")]
#[test]
fn the_entries_follow_the_list_s_order_not_the_parts() {
    let (mut pptx, parts, [first, second, third]) = with_slides();
    set_list(
        &mut pptx,
        &[
            Some(third.1.as_str()),
            Some(first.1.as_str()),
            Some(second.1.as_str()),
        ],
    );
    assert_eq!(
        pptx.slides().unwrap(),
        [
            drawn_with(&third.0, &parts),
            drawn_with(&first.0, &parts),
            drawn_with(&second.0, &parts),
        ]
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_slide_part_the_list_does_not_name_is_not_in_the_result() {
    let (mut pptx, parts, added) = with_slides::<4>();
    let ids: Vec<Option<&str>> = added
        .iter()
        .take(3)
        .map(|(_, id)| Some(id.as_str()))
        .collect();
    set_list(&mut pptx, &ids);
    let slides = pptx.slides().unwrap();
    assert_eq!(slides.len(), 3);
    assert!(
        slides
            .iter()
            .all(|entry| entry.slide != name("/ppt/slides/slide4.xml"))
    );
    assert_eq!(
        slides.last().unwrap(),
        &drawn_with(&name("/ppt/slides/slide3.xml"), &parts)
    );
}

#[test]
fn a_presentation_with_no_slide_list_has_no_slides() {
    let pptx = Pptx::new(PresentationKind::Presentation).unwrap();
    assert!(pptx.presentation().unwrap().list_of_slide_ids.is_none());
    assert_eq!(pptx.slides().unwrap(), []);
}

#[cfg(feature = "transitional")]
#[test]
fn an_entry_whose_relationship_is_missing_is_passed_over() {
    let (mut pptx, parts, [first, second]) = with_slides();
    set_list(
        &mut pptx,
        &[
            Some(first.1.as_str()),
            Some("rId99"),
            None,
            Some(second.1.as_str()),
        ],
    );
    assert_eq!(
        pptx.slides().unwrap(),
        [drawn_with(&first.0, &parts), drawn_with(&second.0, &parts)]
    );
}

#[cfg(feature = "transitional")]
#[test]
fn an_entry_whose_target_the_package_does_not_hold_is_passed_over() {
    let (mut pptx, parts, [only]) = with_slides();
    let main = pptx.presentation_part().clone();
    let absent = pptx
        .package_mut()
        .relate(
            Some(&main),
            &name("/ppt/slides/slide9.xml"),
            slide::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    let external = pptx
        .package_mut()
        .relate_external(
            Some(&main),
            "https://example.com/slide.xml",
            slide::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    set_list(
        &mut pptx,
        &[
            Some(absent.as_str()),
            Some(only.1.as_str()),
            Some(external.as_str()),
        ],
    );
    assert_eq!(pptx.slides().unwrap(), [drawn_with(&only.0, &parts)]);
}

#[cfg(feature = "transitional")]
#[test]
fn a_slide_with_no_layout_is_a_read_error_naming_the_slide() {
    let mut pptx = Pptx::new(PresentationKind::Presentation).unwrap();
    let (part, id) = add_slide(&mut pptx, 1, None);
    set_list(&mut pptx, &[Some(id.as_str())]);
    assert_eq!(
        read_error(pptx.slides().unwrap_err()),
        Some((part, "the slide relates to no slide layout part".to_owned()))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_layout_with_no_master_is_a_read_error_naming_the_layout() {
    let mut pptx = Pptx::new(PresentationKind::Presentation).unwrap();
    let layout = name("/ppt/slideLayouts/slideLayout2.xml");
    slide_layout::store_transitional(
        pptx.package_mut(),
        &layout,
        &SlideLayout::default(),
        WriteOptions::default(),
    )
    .unwrap();
    let (_, id) = add_slide(&mut pptx, 1, Some(&layout));
    set_list(&mut pptx, &[Some(id.as_str())]);
    assert_eq!(
        read_error(pptx.slides().unwrap_err()),
        Some((
            layout,
            "the slide layout relates to no slide master part".to_owned()
        ))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_master_with_no_theme_gives_an_entry_with_no_theme_and_no_error() {
    let mut pptx = Pptx::new(PresentationKind::Presentation).unwrap();
    let master = name("/ppt/slideMasters/slideMaster2.xml");
    let layout = name("/ppt/slideLayouts/slideLayout2.xml");
    slide_master::store_transitional(
        pptx.package_mut(),
        &master,
        &SlideMaster::default(),
        WriteOptions::default(),
    )
    .unwrap();
    slide_layout::store_transitional(
        pptx.package_mut(),
        &layout,
        &SlideLayout::default(),
        WriteOptions::default(),
    )
    .unwrap();
    pptx.package_mut()
        .relate(
            Some(&layout),
            &master,
            slide_master::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    let (slide_part, id) = add_slide(&mut pptx, 1, Some(&layout));
    set_list(&mut pptx, &[Some(id.as_str())]);
    assert_eq!(
        pptx.slides().unwrap(),
        [SlideParts {
            layout,
            master,
            slide: slide_part,
            theme: None,
        }]
    );
}
