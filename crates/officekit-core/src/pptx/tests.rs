#[cfg(feature = "transitional")]
use std::path::Path;

#[cfg(any(feature = "strict", feature = "transitional"))]
use officekit_packaging::package::OpenOptions;
use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::{RelationshipType, Target};
use officekit_parts::kinds::PresentationKind;
use officekit_parts::{slide_layout, slide_master, theme};
use officekit_xml::conformance::ConformanceClass;
#[cfg(feature = "transitional")]
use officekit_xml::mc::MarkupCompatibility;
#[cfg(feature = "transitional")]
use officekit_xml::name::OfficeVersion;
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

    let presentation = text(package, &main);
    assert!(
        presentation.contains("<p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/>"),
        "{presentation}"
    );
    assert!(
        presentation.contains(
            "<p:sldSz cx=\"12192000\" cy=\"6858000\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/>"
        ),
        "{presentation}"
    );
    assert!(!presentation.contains("sldIdLst"), "{presentation}");
    let master_text = text(package, &master);
    assert!(
        master_text.contains("<p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/>"),
        "{master_text}"
    );
    assert!(text(package, &layout).contains("type=\"blank\""));

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
    Pptx::create(&system, path, PresentationKind::MacroEnabledTemplate).unwrap();
    let opened = Pptx::open(&system, path, OpenOptions::default()).unwrap();
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
    let slide = name("/ppt/slides/slide1.xml");
    let xml = "<p:sld xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><p:txBody><a:p><a:r><a:t>The </a:t></a:r><a:r><a:t>fox</a:t></a:r></a:p></p:txBody></p:sld>";
    pptx.package_mut()
        .set_part_bytes(
            &slide,
            "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
            xml.as_bytes(),
        )
        .unwrap();
    pptx.compact_runs(Comments::Keep).unwrap();
    let stored = text(pptx.package(), &slide);
    assert_eq!(stored.matches("<a:r>").count(), 1, "{stored}");
    assert!(stored.contains("<a:t>The fox</a:t>"), "{stored}");
}
