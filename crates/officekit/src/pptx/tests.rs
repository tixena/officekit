use core::slice::from_ref;
#[cfg(feature = "transitional")]
use std::path::Path;

use crate::parts::kinds::PresentationKind;
use crate::parts::part_root::store;
use crate::parts::{slide, slide_layout, slide_master, theme};
use officekit_markup::drawing::{
    TextParagraphs, TextParagraphsContent, TextRun as DrawingRun, TextString,
};
#[cfg(feature = "strict")]
use officekit_markup::presentation::ModificationVerifier;
use officekit_markup::presentation::{
    GroupShapeContent, Presentation, PresentationSlideSlide, Shape, ShapeTextBody,
};
use officekit_xml::conformance::ConformanceClass;
#[cfg(feature = "transitional")]
use officekit_xml::mc::MarkupCompatibility;
#[cfg(feature = "transitional")]
use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::package::OpenOptions;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelationshipType, Target};
#[cfg(feature = "transitional")]
use officekit_xml::ready::ready_now;
use officekit_xml::write::WriteOptions;
#[cfg(feature = "transitional")]
use os_shim::mock::MemorySystem;

use super::Pptx;
use crate::compact::Comments;

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

fn text(package: &Package, part: &PartName) -> String {
    String::from_utf8(package.part_bytes(part).unwrap()).unwrap()
}

/// The width and height of the slides and of the notes pages `presentation` sets.
fn sizes(presentation: &Presentation) -> [(Option<String>, Option<String>); 2] {
    let slides = presentation.slide_size.as_deref().map_or_default(|size| {
        (
            size.cx.as_ref().map(|cx| cx.0.clone()),
            size.cy.as_ref().map(|cy| cy.0.clone()),
        )
    });
    let notes = &presentation.notes_slide_size;
    [
        slides,
        (
            notes.cx.as_ref().map(|cx| cx.0.clone()),
            notes.cy.as_ref().map(|cy| cy.0.clone()),
        ),
    ]
}

/// A pair of an optional width and height.
fn size(cx: &str, cy: &str) -> (Option<String>, Option<String>) {
    (Some(cx.to_owned()), Some(cy.to_owned()))
}

/// A slide whose one shape holds a paragraph of a run for each of `texts`.
fn slide_with_runs(texts: &[&str]) -> PresentationSlideSlide {
    let mut paragraph = TextParagraphs::default();
    paragraph.content = texts
        .iter()
        .map(|text| {
            let mut characters = TextString::default();
            characters.value = (*text).to_owned();
            let mut run = DrawingRun::default();
            run.text_string = Box::new(characters);
            TextParagraphsContent::TextRun(run)
        })
        .collect();
    let mut body = ShapeTextBody::default();
    body.text_paragraphs = vec![paragraph];
    let mut shape = Shape::default();
    shape.text_body = Some(Box::new(body));
    let mut slide_root = PresentationSlideSlide::default();
    slide_root.common_slide_data.shape_tree.content = vec![GroupShapeContent::Shape(shape)];
    slide_root
}

/// The text of each run in the first paragraph of the first shape of `slide_root`.
fn run_texts(slide_root: &PresentationSlideSlide) -> Vec<String> {
    let shape = slide_root
        .common_slide_data
        .shape_tree
        .content
        .iter()
        .find_map(|content| {
            if let GroupShapeContent::Shape(found) = content {
                Some(found)
            } else {
                None
            }
        })
        .unwrap();
    let body = shape.text_body.as_deref().unwrap();
    body.text_paragraphs
        .first()
        .unwrap()
        .content
        .iter()
        .filter_map(|content| {
            if let TextParagraphsContent::TextRun(run) = content {
                Some(run.text_string.value.clone())
            } else {
                None
            }
        })
        .collect()
}

/// The relationships of `source`: id, target, and type.
fn relationships(package: &Package, source: &PartName) -> Vec<(String, Target, String)> {
    package
        .related(Some(source))
        .unwrap()
        .into_iter()
        .map(|related| (related.id, related.target, related.type_uri))
        .collect()
}

#[test]
fn a_new_presentation_holds_a_master_a_blank_layout_and_a_theme_and_no_slide() {
    let pptx = Pptx::new(PresentationKind::Slideshow).unwrap();
    let package = pptx.package();
    let main = name("/ppt/presentation.xml");
    let master = name("/ppt/slideMasters/slideMaster1.xml");
    let layout = name("/ppt/slideLayouts/slideLayout1.xml");
    let theme_part = name("/ppt/theme/theme1.xml");
    assert_eq!(
        package.part_names().unwrap(),
        [
            main.clone(),
            layout.clone(),
            master.clone(),
            theme_part.clone()
        ]
    );
    assert_eq!(pptx.presentation_part(), &main);
    assert_eq!(pptx.kind(), PresentationKind::Slideshow);
    assert_eq!(pptx.conformance_class(), ConformanceClass::Transitional);
    assert_eq!(
        package.content_type(&main),
        Some(PresentationKind::Slideshow.main_content_type())
    );
    assert_eq!(
        package.content_type(&master),
        Some(slide_master::CONTENT_TYPE)
    );
    assert_eq!(
        package.content_type(&layout),
        Some(slide_layout::CONTENT_TYPE)
    );
    assert_eq!(package.content_type(&theme_part), Some(theme::CONTENT_TYPE));

    let presentation = pptx.presentation().unwrap();
    let masters: Vec<(Option<String>, Option<String>)> = presentation
        .list_of_slide_master_ids
        .as_deref()
        .unwrap()
        .slide_master_id
        .iter()
        .map(|id| (id.id.as_ref().map(|value| value.0.clone()), id.r_id.clone()))
        .collect();
    assert_eq!(
        masters,
        [(Some("2147483648".to_owned()), Some("rId1".to_owned()))]
    );
    assert_eq!(
        sizes(&presentation),
        [size("12192000", "6858000"), size("6858000", "9144000")]
    );
    assert!(presentation.list_of_slide_ids.is_none());
    let layouts: Vec<(Option<String>, Option<String>)> =
        slide_master::load(package, &master, OpenOptions::default())
            .unwrap()
            .list_of_slide_layouts
            .unwrap()
            .slide_layout_id
            .into_iter()
            .map(|id| (id.id.map(|value| value.0), id.r_id))
            .collect();
    assert_eq!(
        layouts,
        [(Some("2147483649".to_owned()), Some("rId1".to_owned()))]
    );
    assert_eq!(
        slide_layout::load(package, &layout, OpenOptions::default())
            .unwrap()
            .r#type
            .map(|kind| kind.0),
        Some("blank".to_owned())
    );

    let master_type = RelationshipType::SLIDE_MASTER.transitional.to_owned();
    let theme_type = RelationshipType::THEME.transitional.to_owned();
    let layout_type = RelationshipType::SLIDE_LAYOUT.transitional.to_owned();
    assert_eq!(
        relationships(package, &main),
        [
            (
                "rId1".to_owned(),
                Target::Part(master.clone()),
                master_type.clone()
            ),
            (
                "rId2".to_owned(),
                Target::Part(theme_part.clone()),
                theme_type.clone()
            ),
        ]
    );
    assert_eq!(
        relationships(package, &master),
        [
            ("rId1".to_owned(), Target::Part(layout.clone()), layout_type),
            ("rId2".to_owned(), Target::Part(theme_part), theme_type),
        ]
    );
    assert_eq!(
        relationships(package, &layout),
        [("rId1".to_owned(), Target::Part(master), master_type)]
    );
}

#[test]
fn a_new_add_in_holds_its_presentation_part_alone() {
    let pptx = Pptx::new(PresentationKind::AddIn).unwrap();
    let package = pptx.package();
    let main = name("/ppt/presentation.xml");
    assert_eq!(package.part_names().unwrap(), from_ref(&main));
    assert_eq!(
        package.content_type(&main),
        Some(PresentationKind::AddIn.main_content_type())
    );
    let presentation = pptx.presentation().unwrap();
    assert_eq!(
        sizes(&presentation),
        [size("12192000", "6858000"), size("6858000", "9144000")]
    );
    assert!(presentation.list_of_slide_master_ids.is_none());
    assert_eq!(
        relationships(package, &main),
        Vec::<(String, Target, String)>::new()
    );
}

#[test]
fn the_other_presentation_kinds_keep_their_master_layout_and_theme() {
    for kind in PresentationKind::ALL {
        if kind == PresentationKind::AddIn {
            continue;
        }
        let pptx = Pptx::new(kind).unwrap();
        let names = pptx.package().part_names().unwrap();
        assert_eq!(names.len(), 4, "{kind:?}: {names:?}");
        assert!(
            text(pptx.package(), &name("/ppt/presentation.xml")).contains("sldMasterIdLst"),
            "{kind:?}"
        );
    }
}

#[cfg(feature = "transitional")]
#[test]
fn the_seven_kinds_reopen_with_their_kind() {
    for kind in PresentationKind::ALL {
        let bytes = Pptx::new(kind).unwrap().package().to_bytes().unwrap();
        let reopened = Pptx::from_bytes(bytes, OpenOptions::default()).unwrap();
        assert_eq!(reopened.kind(), kind);
        reopened.presentation().unwrap();
    }
}

#[cfg(feature = "transitional")]
#[test]
fn options_are_those_the_file_was_opened_with() {
    let bytes = Pptx::new(PresentationKind::Presentation)
        .unwrap()
        .package()
        .to_bytes()
        .unwrap();
    let mut options = OpenOptions::default();
    options.markup_compatibility = MarkupCompatibility::Process {
        target: OfficeVersion::Office2016,
    };
    assert_eq!(Pptx::from_bytes(bytes, options).unwrap().options(), options);
}

#[cfg(feature = "transitional")]
#[test]
fn create_writes_the_new_presentation_through_the_system() {
    let system = MemorySystem::new().with_dir("/out").unwrap();
    let path = Path::new("/out/new.potm");
    ready_now(Pptx::create(
        &system,
        path,
        PresentationKind::MacroEnabledTemplate,
    ))
    .unwrap();
    let opened = ready_now(Pptx::open(&system, path, OpenOptions::default())).unwrap();
    assert_eq!(opened.kind(), PresentationKind::MacroEnabledTemplate);
}

#[cfg(feature = "strict")]
#[test]
fn a_new_presentation_saved_strict_reopens_strict() {
    let strict = Pptx::new(PresentationKind::Presentation)
        .unwrap()
        .to_bytes_strict()
        .unwrap();
    let pptx = Pptx::from_bytes(strict, OpenOptions::default()).unwrap();
    assert_eq!(pptx.conformance_class(), ConformanceClass::Strict);
    let presentation = text(pptx.package(), pptx.presentation_part());
    assert!(
        presentation.contains("conformance=\"strict\""),
        "{presentation}"
    );
    assert!(
        presentation.contains("xmlns:p=\"http://purl.oclc.org/ooxml/presentationml/main\""),
        "{presentation}"
    );
    let theme_text = text(pptx.package(), &name("/ppt/theme/theme1.xml"));
    assert!(
        theme_text.contains("xmlns:a=\"http://purl.oclc.org/ooxml/drawingml/main\""),
        "{theme_text}"
    );
    for (_, _, type_uri) in relationships(pptx.package(), pptx.presentation_part()) {
        assert!(
            type_uri.starts_with("http://purl.oclc.org/ooxml/"),
            "{type_uri}"
        );
    }
}

#[test]
fn compact_runs_merges_neighbouring_runs_of_a_slide_and_keeps_the_text() {
    let mut pptx = Pptx::new(PresentationKind::Slideshow).unwrap();
    let part = name("/ppt/slides/slide1.xml");
    let class = pptx.conformance_class();
    store(
        pptx.package_mut(),
        &part,
        slide::CONTENT_TYPE,
        &slide_with_runs(&["The ", "fox"]),
        class,
        WriteOptions::default(),
    )
    .unwrap();
    pptx.compact_runs(Comments::Keep).unwrap();
    let stored = slide::load(pptx.package(), &part, OpenOptions::default()).unwrap();
    assert_eq!(run_texts(&stored), ["The fox"]);
}

#[cfg(feature = "strict")]
#[test]
fn strict_refusals_names_a_presentation_s_legacy_verifier() {
    let mut pptx = Pptx::new(PresentationKind::Presentation).unwrap();
    assert_eq!(pptx.strict_refusals().unwrap(), []);
    let mut verifier = ModificationVerifier::default();
    verifier.hash_data = Some("AAAA".to_owned());
    let mut presentation = pptx.presentation().unwrap();
    presentation.modification_verifier = Some(Box::new(verifier));
    pptx.set_presentation(&presentation).unwrap();
    assert_eq!(
        pptx.strict_refusals().unwrap(),
        [crate::StrictRefusal {
            count: 1,
            part: name("/ppt/presentation.xml"),
            prefix: "p",
            tag: "modifyVerifier/@hashData",
        }]
    );
    pptx.to_bytes_strict().unwrap_err();
}
