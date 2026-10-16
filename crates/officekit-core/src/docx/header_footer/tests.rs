use crate::docx::Docx;
use crate::error::PackageError;
use crate::markup::wordprocessing::{
    BlockContent, DocumentFinalSectionProperties, Footer, Header, HeaderOrFooterTypeMember,
    Paragraph, SectionPropertiesContent,
};
use crate::packaging::package::{OpenOptions, Package};
use crate::packaging::part_name::PartName;
use crate::packaging::relationship::{RelationshipType, Target};
use crate::parts::kinds::WordprocessingKind;
use crate::parts::{document_settings, footer, header};
#[cfg(feature = "strict")]
use crate::xml::conformance::ConformanceClass;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

fn text(docx: &Docx, part: &str) -> String {
    String::from_utf8(docx.package().part_bytes(&name(part)).unwrap()).unwrap()
}

fn one_paragraph() -> Vec<BlockContent> {
    vec![BlockContent::Paragraph(Paragraph::default())]
}

fn a_header() -> Header {
    Header {
        content: one_paragraph(),
        ..Header::default()
    }
}

fn a_footer() -> Footer {
    Footer {
        content: one_paragraph(),
        ..Footer::default()
    }
}

/// A package whose main part is `body` and whose root declares the relationships namespace under
/// `prefix`, with one header and one footer part per entry of `parts` as `(part, id)`.
fn package_with(body: &str, prefix: &str, parts: &[(&str, &str)], settings: bool) -> Package {
    let mut package = Package::new();
    let main = name("/word/document.xml");
    let document = format!(
        "<w:document xmlns:w=\"{W}\" xmlns:{prefix}=\"{R}\"><w:body>{body}</w:body></w:document>"
    );
    package
        .set_part_bytes(
            &main,
            WordprocessingKind::Document.main_content_type(),
            document.as_bytes(),
        )
        .unwrap();
    package
        .relate(None, &main, RelationshipType::OFFICE_DOCUMENT.transitional)
        .unwrap();
    for (part, _) in parts {
        let (content_type, relationship, root) = if part.contains("header") {
            (header::CONTENT_TYPE, RelationshipType::HEADER, "hdr")
        } else {
            (footer::CONTENT_TYPE, RelationshipType::FOOTER, "ftr")
        };
        let target = name(part);
        let xml = format!("<w:{root} xmlns:w=\"{W}\"><w:p/></w:{root}>");
        package
            .set_part_bytes(&target, content_type, xml.as_bytes())
            .unwrap();
        package
            .relate(Some(&main), &target, relationship.transitional)
            .unwrap();
    }
    if settings {
        let target = name("/word/settings.xml");
        package
            .set_part_bytes(
                &target,
                document_settings::CONTENT_TYPE,
                format!("<w:settings xmlns:w=\"{W}\"><w:zoom w:percent=\"100\"/></w:settings>")
                    .as_bytes(),
            )
            .unwrap();
        package
            .relate(
                Some(&main),
                &target,
                RelationshipType::SETTINGS.transitional,
            )
            .unwrap();
    }
    package
}

/// One section: a default header, a default footer, and a first-page footer.
fn pagare() -> Docx {
    let section = "<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId1\"/><w:footerReference w:type=\"default\" r:id=\"rId2\"/><w:footerReference w:type=\"first\" r:id=\"rId3\"/><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr>";
    let package = package_with(
        &format!("<w:p/>{section}"),
        "r",
        &[
            ("/word/header1.xml", "rId1"),
            ("/word/footer1.xml", "rId2"),
            ("/word/footer2.xml", "rId3"),
        ],
        true,
    );
    Docx::from_package(package, OpenOptions::default()).unwrap()
}

fn body_section(docx: &Docx) -> DocumentFinalSectionProperties {
    let document = docx.main_document().unwrap();
    *document
        .body
        .unwrap()
        .document_final_section_properties
        .unwrap()
}

/// The `(element, type, id)` of each reference of the section, in order.
fn references(section: &DocumentFinalSectionProperties) -> Vec<(&'static str, String, String)> {
    section
        .content
        .iter()
        .filter_map(|item| match item {
            SectionPropertiesContent::HeaderReference(found) => Some((
                "header",
                found.r#type.as_ref().unwrap().0.clone(),
                found.id.clone().unwrap(),
            )),
            SectionPropertiesContent::FooterReference(found) => Some((
                "footer",
                found.r#type.as_ref().unwrap().0.clone(),
                found.id.clone().unwrap(),
            )),
            SectionPropertiesContent::Other(_) => None,
        })
        .collect()
}

fn related_ids(docx: &Docx) -> Vec<String> {
    docx.package()
        .related(Some(docx.main_document_part()))
        .unwrap()
        .into_iter()
        .map(|part| part.id)
        .collect()
}

fn is_read(error: &PackageError) -> bool {
    matches!(error, PackageError::Read { part, reason }
        if part.as_str() == "/word/document.xml" && !reason.is_empty())
}

fn entry(element: &'static str, kind: &str, id: &str) -> (&'static str, String, String) {
    (element, kind.to_owned(), id.to_owned())
}

#[test]
fn a_first_page_header_goes_after_the_references_and_turns_on_title_page() {
    let mut docx = pagare();
    let part = docx
        .add_header(0, HeaderOrFooterTypeMember::First, &a_header())
        .unwrap();
    assert_eq!(part, name("/word/header2.xml"));
    let section = body_section(&docx);
    assert_eq!(
        references(&section),
        vec![
            entry("header", "default", "rId1"),
            entry("footer", "default", "rId2"),
            entry("footer", "first", "rId3"),
            entry("header", "first", "rId5"),
        ]
    );
    assert!(section.different_first_page_headers_and_footers.is_some());
    assert!(text(&docx, "/word/document.xml").contains("<w:titlePg/>"));
    assert_eq!(
        docx.package().content_type(&part),
        Some(header::CONTENT_TYPE)
    );
}

#[test]
fn the_new_reference_goes_before_the_other_section_properties() {
    let mut docx = pagare();
    docx.add_footer(0, HeaderOrFooterTypeMember::Even, &a_footer())
        .unwrap();
    let xml = text(&docx, "/word/document.xml");
    let reference = xml.find("w:type=\"even\"").unwrap();
    assert!(reference < xml.find("<w:pgSz").unwrap());
    assert!(xml.find("w:type=\"first\"").unwrap() < reference);
}

#[test]
fn replacing_the_default_header_changes_the_id_and_removes_the_old_part() {
    let mut docx = pagare();
    let part = docx
        .add_header(0, HeaderOrFooterTypeMember::Default, &a_header())
        .unwrap();
    assert_eq!(part, name("/word/header2.xml"));
    let section = body_section(&docx);
    assert_eq!(references(&section)[0], entry("header", "default", "rId5"));
    assert_eq!(references(&section).len(), 3);
    assert!(!docx.package().contains(&name("/word/header1.xml")));
    assert!(!related_ids(&docx).contains(&"rId1".to_owned()));
    assert!(section.different_first_page_headers_and_footers.is_none());
}

#[test]
fn an_even_footer_creates_the_settings_part_when_the_document_has_none() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let part = docx
        .add_footer(0, HeaderOrFooterTypeMember::Even, &a_footer())
        .unwrap();
    assert_eq!(part, name("/word/footer1.xml"));
    let settings = name("/word/settings1.xml");
    assert!(
        text(&docx, "/word/settings1.xml").contains("<w:evenAndOddHeaders/>"),
        "{}",
        text(&docx, "/word/settings1.xml")
    );
    let related = docx
        .package()
        .related_by_type(Some(docx.main_document_part()), &RelationshipType::SETTINGS)
        .unwrap();
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].target, Target::Part(settings.clone()));
    assert_eq!(
        docx.package().content_type(&settings),
        Some(document_settings::CONTENT_TYPE)
    );
}

#[test]
fn an_even_header_turns_on_the_flag_in_the_settings_part_the_document_has() {
    let mut docx = pagare();
    docx.add_header(0, HeaderOrFooterTypeMember::Even, &a_header())
        .unwrap();
    let settings = text(&docx, "/word/settings.xml");
    assert!(settings.contains("<w:evenAndOddHeaders/>"), "{settings}");
    assert!(
        settings.contains("<w:zoom w:percent=\"100\"/>"),
        "{settings}"
    );
    assert!(settings.find("<w:zoom").unwrap() < settings.find("<w:evenAndOddHeaders").unwrap());
    assert!(!docx.package().contains(&name("/word/settings1.xml")));
}

#[test]
fn a_new_document_gets_a_body_section_and_declares_the_relationships_prefix() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    assert!(!text(&docx, "/word/document.xml").contains("sectPr"));
    docx.add_header(0, HeaderOrFooterTypeMember::Default, &a_header())
        .unwrap();
    let xml = text(&docx, "/word/document.xml");
    assert!(xml.contains(&format!("xmlns:r=\"{R}\"")), "{xml}");
    assert!(
        xml.contains("<w:sectPr><w:headerReference r:id=\"rId1\" w:type=\"default\"/></w:sectPr>"),
        "{xml}"
    );
}

#[test]
fn the_documents_own_relationships_prefix_is_used() {
    let package = package_with("<w:p/><w:sectPr/>", "rel", &[], false);
    let mut docx = Docx::from_package(package, OpenOptions::default()).unwrap();
    docx.add_header(0, HeaderOrFooterTypeMember::Default, &a_header())
        .unwrap();
    let xml = text(&docx, "/word/document.xml");
    assert!(xml.contains("rel:id=\"rId1\""), "{xml}");
    assert!(!xml.contains("xmlns:r="), "{xml}");
}

#[test]
fn a_section_past_the_last_is_a_read_error_and_nothing_changes() {
    let mut docx = pagare();
    let before = docx.package().to_bytes().unwrap();
    let error = docx
        .add_header(1, HeaderOrFooterTypeMember::Default, &a_header())
        .unwrap_err();
    assert!(is_read(&error), "{error:?}");
    let removal = docx
        .remove_footer(1, HeaderOrFooterTypeMember::Default)
        .unwrap_err();
    assert!(is_read(&removal), "{removal:?}");
    assert_eq!(docx.package().to_bytes().unwrap(), before);
}

#[test]
fn a_header_or_footer_with_no_block_is_refused_before_anything_changes() {
    let mut docx = pagare();
    let before = docx.package().to_bytes().unwrap();
    let error = docx
        .add_header(0, HeaderOrFooterTypeMember::First, &Header::default())
        .unwrap_err();
    assert!(is_read(&error), "{error:?}");
    let footer_error = docx
        .add_footer(0, HeaderOrFooterTypeMember::First, &Footer::default())
        .unwrap_err();
    assert!(is_read(&footer_error), "{footer_error:?}");
    assert_eq!(docx.package().to_bytes().unwrap(), before);
}

#[test]
fn removing_a_footer_removes_its_part_and_leaves_the_flags() {
    let mut docx = pagare();
    docx.add_footer(0, HeaderOrFooterTypeMember::Even, &a_footer())
        .unwrap();
    let removed = docx
        .remove_footer(0, HeaderOrFooterTypeMember::Even)
        .unwrap();
    assert_eq!(removed, Some(name("/word/footer3.xml")));
    assert!(!docx.package().contains(&name("/word/footer3.xml")));
    assert!(
        text(&docx, "/word/settings.xml").contains("<w:evenAndOddHeaders/>"),
        "the flag stays"
    );
    let removed_first = docx
        .remove_footer(0, HeaderOrFooterTypeMember::First)
        .unwrap();
    assert_eq!(removed_first, Some(name("/word/footer2.xml")));
    assert_eq!(
        references(&body_section(&docx)),
        vec![
            entry("header", "default", "rId1"),
            entry("footer", "default", "rId2")
        ]
    );
}

#[test]
fn removing_a_reference_the_section_does_not_have_returns_none() {
    let mut docx = pagare();
    let before = docx.package().to_bytes().unwrap();
    assert_eq!(
        docx.remove_header(0, HeaderOrFooterTypeMember::Even)
            .unwrap(),
        None
    );
    assert_eq!(docx.package().to_bytes().unwrap(), before);
}

#[test]
fn removing_a_first_page_header_leaves_title_page_on() {
    let mut docx = pagare();
    docx.add_header(0, HeaderOrFooterTypeMember::First, &a_header())
        .unwrap();
    docx.remove_header(0, HeaderOrFooterTypeMember::First)
        .unwrap();
    assert!(text(&docx, "/word/document.xml").contains("<w:titlePg/>"));
}

#[test]
fn a_part_two_sections_share_stays_until_the_last_reference_goes() {
    let first = "<w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId1\"/></w:sectPr></w:pPr></w:p>";
    let second = "<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId1\"/></w:sectPr>";
    let package = package_with(
        &format!("{first}{second}"),
        "r",
        &[("/word/header1.xml", "rId1")],
        false,
    );
    let mut docx = Docx::from_package(package, OpenOptions::default()).unwrap();
    assert_eq!(
        docx.remove_header(0, HeaderOrFooterTypeMember::Default)
            .unwrap(),
        None
    );
    assert!(docx.package().contains(&name("/word/header1.xml")));
    assert_eq!(
        docx.remove_header(1, HeaderOrFooterTypeMember::Default)
            .unwrap(),
        Some(name("/word/header1.xml"))
    );
    assert!(!docx.package().contains(&name("/word/header1.xml")));
}

#[test]
fn sections_inside_block_level_content_controls_count_in_document_order() {
    let control =
        "<w:sdt><w:sdtContent><w:p><w:pPr><w:sectPr/></w:pPr></w:p></w:sdtContent></w:sdt>";
    let plain = "<w:p><w:pPr><w:sectPr/></w:pPr></w:p>";
    let package = package_with(
        &format!("{plain}{control}<w:p/><w:sectPr/>"),
        "r",
        &[],
        false,
    );
    let mut docx = Docx::from_package(package, OpenOptions::default()).unwrap();
    docx.add_header(1, HeaderOrFooterTypeMember::Default, &a_header())
        .unwrap();
    let xml = text(&docx, "/word/document.xml");
    let reference = xml.find("<w:headerReference").unwrap();
    let control_at = xml.find("<w:sdt>").unwrap();
    assert!(
        control_at < reference && reference < xml.find("</w:sdt>").unwrap(),
        "{xml}"
    );
    docx.add_footer(2, HeaderOrFooterTypeMember::Default, &a_footer())
        .unwrap();
    let error = docx
        .add_footer(3, HeaderOrFooterTypeMember::Default, &a_footer())
        .unwrap_err();
    assert!(is_read(&error), "{error:?}");
}

#[cfg(feature = "strict")]
#[test]
fn a_strict_file_gets_strict_relationship_types_and_namespace() {
    let bytes = pagare().to_bytes_strict().unwrap();
    let mut docx = Docx::from_bytes(bytes, OpenOptions::default()).unwrap();
    assert_eq!(docx.conformance_class(), ConformanceClass::Strict);
    let part = docx
        .add_header(0, HeaderOrFooterTypeMember::First, &a_header())
        .unwrap();
    let related = docx
        .package()
        .related(Some(docx.main_document_part()))
        .unwrap();
    let found = related
        .iter()
        .find(|candidate| candidate.target == Target::Part(part.clone()))
        .unwrap();
    assert_eq!(found.type_uri, RelationshipType::HEADER.strict);
}
